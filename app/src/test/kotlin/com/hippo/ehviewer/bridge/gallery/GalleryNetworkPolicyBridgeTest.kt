package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryNetworkPolicyBridgeTest {
  @Test
  fun metadataPlanPreservesTwentyFiveByFiveWavesAndFiveSecondDelay() {
    val policy = invokeMetadataPolicy(itemCount = 126)

    assertEquals(25, policy.itemsPerRequest)
    assertEquals(5, policy.requestsPerWave)
    assertEquals(6, policy.requestCount)
    assertEquals(2, policy.waveCount)
    assertEquals(5_000L, policy.waveDelayMillis)
    assertEquals(
      listOf(listOf(25, 25, 25, 25, 25), listOf(1)),
      policy.toRequestWaves((0 until 126).toList()).map { wave -> wave.map { it.size } },
    )
  }

  @Test
  fun metadataBoundaryCasesKeepZeroAndExactWaveLimits() {
    val cases =
      listOf(
        0 to (0 to 0),
        1 to (1 to 1),
        25 to (1 to 1),
        26 to (2 to 1),
        125 to (5 to 1),
        126 to (6 to 2),
      )

    cases.forEach { (itemCount, counts) ->
      val policy = invokeMetadataPolicy(itemCount)
      assertEquals(counts.first, policy.requestCount)
      assertEquals(counts.second, policy.waveCount)
    }
  }

  @Test
  fun archivePlansFreezeDirectRetryAndHathNoRetry() {
    assertEquals(directArchivePolicy, invokeArchivePolicy(isHath = false))
    assertEquals(hathArchivePolicy, invokeArchivePolicy(isHath = true))
  }

  @Test
  fun eachWholeOperationUsesOneNativeCallWithExactCborRequest() {
    fixtures.forEach { fixture ->
      var calls = 0
      val actual =
        when (fixture.request.kind) {
          GalleryNetworkPolicyKind.METADATA ->
            planGalleryMetadataPolicy(requireNotNull(fixture.request.itemCount)) {
              requestBuffer,
              requestSize,
              responseBuffer ->
              calls += 1
              respond(fixture, requestBuffer, requestSize, responseBuffer)
            }
          GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD ->
            planGalleryArchiveDownloadPolicy(requireNotNull(fixture.request.isHath)) {
              requestBuffer,
              requestSize,
              responseBuffer ->
              calls += 1
              respond(fixture, requestBuffer, requestSize, responseBuffer)
            }
        }

      assertEquals(fixture.payload(), actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun validatorRejectsEnvelopeShapeAndPolicyDrift() {
    val request = GalleryNetworkPolicyRequest.metadata(126)
    val valid = metadataEnvelope
    assertFailsWith<IllegalArgumentException> {
      valid.copy(version = 2).toGalleryNetworkPolicyPlan(request)
    }
    assertFailsWith<IllegalArgumentException> {
      valid.copy(module = "other").toGalleryNetworkPolicyPlan(request)
    }
    assertFailsWith<RustFailureException> {
      ApiEnvelope<GalleryNetworkPolicyPlan>(
          version = 1,
          module = GALLERY_NETWORK_POLICY_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toGalleryNetworkPolicyPlan(request)
    }
    assertFailsWith<IllegalArgumentException> {
      valid
        .copy(
          payload =
            metadataPolicy.copy(
              metadata = requireNotNull(metadataPolicy.metadata).copy(itemsPerRequest = 0)
            )
        )
        .toGalleryNetworkPolicyPlan(request)
    }
    assertFailsWith<IllegalArgumentException> {
      valid
        .copy(payload = metadataPolicy.copy(archiveDownload = hathArchivePolicy))
        .toGalleryNetworkPolicyPlan(request)
    }
  }

  @Test
  fun requestConstructorRejectsInvalidOptionalShapesAndNegativeCounts() {
    val cases =
      listOf(
        Triple(GalleryNetworkPolicyKind.METADATA, null, null),
        Triple(GalleryNetworkPolicyKind.METADATA, -1, null),
        Triple(GalleryNetworkPolicyKind.METADATA, 1, false),
        Triple(GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD, null, null),
        Triple(GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD, 1, null),
        Triple(GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD, 1, false),
      )

    cases.forEach { (kind, itemCount, isHath) ->
      assertFailsWith<IllegalArgumentException> {
        GalleryNetworkPolicyRequest(1, kind, itemCount, isHath)
      }
    }
  }

  private fun invokeMetadataPolicy(itemCount: Int): GalleryMetadataPolicyPlan {
    val request = GalleryNetworkPolicyRequest.metadata(itemCount)
    val payload = metadataPlan(itemCount)
    return planGalleryMetadataPolicy(itemCount) { requestBuffer, requestSize, responseBuffer ->
      assertEquals(request, decodeRequest(requestBuffer, requestSize))
      encodeResponse(ApiEnvelope(1, GALLERY_NETWORK_POLICY_MODULE, payload), responseBuffer)
    }
  }

  private fun invokeArchivePolicy(isHath: Boolean): GalleryArchiveDownloadPolicyPlan {
    val request = GalleryNetworkPolicyRequest.archiveDownload(isHath)
    val payload = archivePlan(if (isHath) hathArchivePolicy else directArchivePolicy)
    return planGalleryArchiveDownloadPolicy(isHath) { requestBuffer, requestSize, responseBuffer ->
      assertEquals(request, decodeRequest(requestBuffer, requestSize))
      encodeResponse(ApiEnvelope(1, GALLERY_NETWORK_POLICY_MODULE, payload), responseBuffer)
    }
  }

  private fun respond(
    fixture: WireFixture,
    requestBuffer: java.nio.ByteBuffer,
    requestSize: Int,
    responseBuffer: java.nio.ByteBuffer,
  ): Int {
    assertEquals(fixture.request, decodeRequest(requestBuffer, requestSize))
    return encodeResponse(fixture.envelope, responseBuffer)
  }

  private fun decodeRequest(buffer: java.nio.ByteBuffer, size: Int): GalleryNetworkPolicyRequest {
    val bytes = ByteArray(size).also { buffer.get(it) }
    return rustCbor.decodeFromByteArray(bytes)
  }

  private fun encodeResponse(
    envelope: ApiEnvelope<GalleryNetworkPolicyPlan>,
    buffer: java.nio.ByteBuffer,
  ): Int {
    val bytes = rustCbor.encodeToByteArray(envelope)
    buffer.put(bytes)
    return bytes.size
  }

  private data class WireFixture(
    val request: GalleryNetworkPolicyRequest,
    val envelope: ApiEnvelope<GalleryNetworkPolicyPlan>,
  ) {
    fun payload(): Any = requireNotNull(envelope.payload).run { metadata ?: archiveDownload!! }
  }

  private companion object {
    fun metadataPlan(itemCount: Int): GalleryNetworkPolicyPlan {
      val requestCount = itemCount / 25 + if (itemCount % 25 == 0) 0 else 1
      val waveCount = requestCount / 5 + if (requestCount % 5 == 0) 0 else 1
      return GalleryNetworkPolicyPlan(
        GalleryNetworkPolicyKind.METADATA,
        GalleryMetadataPolicyPlan(itemCount, 25, 5, requestCount, waveCount, 5_000),
        null,
      )
    }

    fun archivePlan(policy: GalleryArchiveDownloadPolicyPlan) =
      GalleryNetworkPolicyPlan(GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD, null, policy)

    val directArchivePolicy =
      GalleryArchiveDownloadPolicyPlan(
        false,
        listOf(GalleryArchiveAttemptPlan(0), GalleryArchiveAttemptPlan(1_000)),
        GalleryArchiveParsedOutcome.RETURN_URL,
        GalleryArchiveNullOutcome.ARCHIVE_UNAVAILABLE,
      )
    val hathArchivePolicy =
      GalleryArchiveDownloadPolicyPlan(
        true,
        listOf(GalleryArchiveAttemptPlan(0)),
        GalleryArchiveParsedOutcome.RETURN_NULL,
        GalleryArchiveNullOutcome.RETURN_NULL,
      )
    val metadataPolicy = metadataPlan(126)
    val metadataEnvelope = ApiEnvelope(1, GALLERY_NETWORK_POLICY_MODULE, metadataPolicy)
    val fixtures =
      listOf(
        WireFixture(GalleryNetworkPolicyRequest.metadata(126), metadataEnvelope),
        WireFixture(
          GalleryNetworkPolicyRequest.archiveDownload(false),
          ApiEnvelope(1, GALLERY_NETWORK_POLICY_MODULE, archivePlan(directArchivePolicy)),
        ),
        WireFixture(
          GalleryNetworkPolicyRequest.archiveDownload(true),
          ApiEnvelope(1, GALLERY_NETWORK_POLICY_MODULE, archivePlan(hathArchivePolicy)),
        ),
      )
  }
}
