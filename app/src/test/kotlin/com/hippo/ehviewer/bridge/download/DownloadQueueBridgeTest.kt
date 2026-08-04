package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class DownloadQueueBridgeTest {
  @Test
  fun validatesACompleteRustQueuePlan() {
    val plan =
      DownloadQueuePlan(
        snapshot =
          DownloadQueueSnapshot(
            entries =
              listOf(
                DownloadEntry(1, DownloadState.NONE),
                DownloadEntry(2, DownloadState.DOWNLOADING),
                DownloadEntry(3, DownloadState.WAITING),
              ),
            waiting = listOf(3),
            current = 2,
          ),
        changes =
          listOf(
            DownloadStateChange(1, DownloadState.DOWNLOADING, DownloadState.NONE),
            DownloadStateChange(2, DownloadState.WAITING, DownloadState.DOWNLOADING),
          ),
        startGalleryId = 2,
        cancelGalleryId = 1,
      )

    val decoded = ApiEnvelope(version = 1, module = "downloadQueue", payload = plan)

    val previous =
      DownloadQueueSnapshot(
        entries =
          listOf(
            DownloadEntry(1, DownloadState.DOWNLOADING),
            DownloadEntry(2, DownloadState.WAITING),
            DownloadEntry(3, DownloadState.WAITING),
          ),
        waiting = listOf(2, 3),
        current = 1,
      )
    assertEquals(plan, decoded.toDownloadQueuePlan(previous))
  }

  @Test
  fun decodesTheRustGeneratedDownloadQueueGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<DownloadQueuePlan>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val plan = envelope.toDownloadQueuePlan()

    assertEquals(7, plan.startGalleryId)
    assertEquals(7, plan.snapshot.current)
    assertEquals(DownloadState.DOWNLOADING, plan.snapshot.entries.single().state)
    assertEquals(DownloadState.NONE, plan.changes.single().previous)
  }

  @Test
  fun encodesTheRustGeneratedDownloadQueueRequestGoldenFixtures() {
    val request =
      DownloadQueueRequest(
        version = 1,
        snapshot =
          DownloadQueueSnapshot(
            entries = listOf(DownloadEntry(7, DownloadState.DOWNLOADING)),
            waiting = emptyList(),
            current = 7,
          ),
        command = DownloadQueueCommand.startAll(),
      )

    val encoded = rustCbor.encodeToByteArray(request)
    val encodedHex =
      encoded.joinToString(separator = "") { it.toUByte().toString(16).padStart(2, '0') }

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encodedHex)

    val recovery =
      DownloadQueueRecoveryRequest(
        version = 1,
        entries = listOf(DownloadEntry(7, DownloadState.DOWNLOADING)),
      )
    val recoveryHex =
      rustCbor.encodeToByteArray(recovery).joinToString(separator = "") {
        it.toUByte().toString(16).padStart(2, '0')
      }
    assertEquals(V1_RECOVERY_REQUEST_GOLDEN_CBOR_HEX, recoveryHex)
  }

  @Test
  fun rejectsWrongEnvelopeAndPreservesTypedRustFailure() {
    val plan = idlePlan()
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "downloadQueue", payload = plan).toDownloadQueuePlan()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = plan).toDownloadQueuePlan()
    }

    val failure = RustError(RustErrorKind.ERROR, "invalid queue")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<DownloadQueuePlan>(version = 1, module = "downloadQueue", error = failure)
          .toDownloadQueuePlan()
      }
    assertEquals(failure, error.failure)
  }

  @Test
  fun commandFactoriesEnforceTaggedUnionAndIdentifierRules() {
    assertEquals(7, DownloadQueueCommand.startOne(7).galleryId)
    assertEquals(listOf(3L, 2L), DownloadQueueCommand.startRange(listOf(3, 2)).galleryIds)
    assertEquals(DownloadCommandKind.START_ALL, DownloadQueueCommand.startAll().kind)
    assertEquals(DownloadCommandKind.ACTIVATE_NEXT, DownloadQueueCommand.activateNext().kind)
    assertEquals(DownloadCommandKind.STOP_ONE, DownloadQueueCommand.stopOne(1).kind)
    assertEquals(listOf(2L, 1L), DownloadQueueCommand.stopRange(listOf(2, 1)).galleryIds)
    assertEquals(DownloadCommandKind.STOP_ALL, DownloadQueueCommand.stopAll().kind)
    assertEquals(false, DownloadQueueCommand.finish(1, failed = false).failed)

    assertFailsWith<IllegalArgumentException> { DownloadQueueCommand.startOne(0) }
    assertFailsWith<IllegalArgumentException> {
      DownloadQueueCommand.startRange(listOf(1, 1))
    }
    assertFailsWith<IllegalArgumentException> {
      DownloadQueueCommand.startRange(listOf(-1))
    }
  }

  @Test
  fun validatesProcessRecoveryAgainstPersistedEntries() {
    val previous =
      listOf(
        DownloadEntry(1, DownloadState.WAITING),
        DownloadEntry(2, DownloadState.DOWNLOADING),
        DownloadEntry(3, DownloadState.FINISHED),
      )
    val recovered =
      DownloadQueuePlan(
        snapshot =
          DownloadQueueSnapshot(
            entries =
              listOf(
                DownloadEntry(1, DownloadState.NONE),
                DownloadEntry(2, DownloadState.NONE),
                DownloadEntry(3, DownloadState.FINISHED),
              ),
            waiting = emptyList(),
          ),
        changes =
          listOf(
            DownloadStateChange(1, DownloadState.WAITING, DownloadState.NONE),
            DownloadStateChange(2, DownloadState.DOWNLOADING, DownloadState.NONE),
          ),
      )
    val envelope = ApiEnvelope(version = 1, module = "downloadQueue", payload = recovered)

    assertEquals(recovered, envelope.toRecoveredDownloadQueuePlan(previous))
    assertFailsWith<IllegalArgumentException> {
      envelope
        .copy(payload = recovered.copy(startGalleryId = 1))
        .toRecoveredDownloadQueuePlan(previous)
    }
    assertFailsWith<IllegalArgumentException> {
      envelope
        .copy(
          payload =
            recovered.copy(
              snapshot = recovered.snapshot.copy(entries = recovered.snapshot.entries.reversed())
            )
        )
        .toRecoveredDownloadQueuePlan(previous)
    }
    assertFailsWith<IllegalArgumentException> {
      envelope
        .copy(
          payload =
            recovered.copy(
              snapshot =
                recovered.snapshot.copy(
                  entries =
                    recovered.snapshot.entries.map {
                      if (it.galleryId == 3L) it.copy(state = DownloadState.NONE) else it
                    }
                ),
              changes =
                recovered.changes +
                  DownloadStateChange(3, DownloadState.FINISHED, DownloadState.NONE),
            )
        )
        .toRecoveredDownloadQueuePlan(previous)
    }
  }

  @Test
  fun invokesTheSelectedNativeBoundaryAndValidatesItsResponseSize() {
    val request = byteArrayOf(1, 2, 3)
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(version = 1, module = "downloadQueue", payload = idlePlan())
      )
    var invocationCount = 0

    val decoded =
      invokeDownloadQueue(request) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        assertEquals(request.size, requestSize)
        val actualRequest = ByteArray(requestSize)
        requestBuffer.get(actualRequest)
        assertTrue(request.contentEquals(actualRequest))
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(idlePlan(), decoded.payload)
    assertFailsWith<IllegalArgumentException> {
      invokeDownloadQueue(request) { _, _, _ -> 0 }
    }
    assertFailsWith<IllegalArgumentException> {
      invokeDownloadQueue(request) { _, _, responseBuffer -> responseBuffer.capacity() + 1 }
    }
  }

  @Test
  fun rejectsMalformedSnapshotChangesAndEffects() {
    val duplicateEntries =
      idlePlan()
        .copy(
          snapshot =
            DownloadQueueSnapshot(
              entries =
                listOf(
                  DownloadEntry(1, DownloadState.NONE),
                  DownloadEntry(1, DownloadState.NONE),
                ),
              waiting = emptyList(),
            )
        )
    assertInvalid(duplicateEntries)

    val orphanWaiting =
      idlePlan()
        .copy(
          snapshot =
            DownloadQueueSnapshot(
              entries = listOf(DownloadEntry(1, DownloadState.WAITING)),
              waiting = emptyList(),
            )
        )
    assertInvalid(orphanWaiting)

    val missingWaitingEntry =
      idlePlan()
        .copy(
          snapshot =
            DownloadQueueSnapshot(
              entries = listOf(DownloadEntry(1, DownloadState.NONE)),
              waiting = listOf(2),
            )
        )
    assertInvalid(missingWaitingEntry)

    val activeWithoutCurrent =
      idlePlan()
        .copy(
          snapshot =
            DownloadQueueSnapshot(
              entries = listOf(DownloadEntry(1, DownloadState.DOWNLOADING)),
              waiting = emptyList(),
            )
        )
    assertInvalid(activeWithoutCurrent)

    val duplicateChanges =
      idlePlan()
        .copy(
          changes =
            listOf(
              DownloadStateChange(1, DownloadState.WAITING, DownloadState.NONE),
              DownloadStateChange(1, DownloadState.FAILED, DownloadState.NONE),
            )
        )
    assertInvalid(duplicateChanges)

    val mismatchedChange =
      idlePlan()
        .copy(changes = listOf(DownloadStateChange(1, DownloadState.NONE, DownloadState.FAILED)))
    assertInvalid(mismatchedChange)

    assertInvalid(idlePlan().copy(startGalleryId = 1))
    assertInvalid(idlePlan().copy(cancelGalleryId = 2))
    assertInvalid(idlePlan().copy(cancelGalleryId = 1))
    assertInvalid(
      idlePlan()
        .copy(
          changes = listOf(DownloadStateChange(1, DownloadState.WAITING, DownloadState.NONE)),
          cancelGalleryId = 1,
        )
    )

    val active =
      DownloadQueuePlan(
        snapshot =
          DownloadQueueSnapshot(
            entries = listOf(DownloadEntry(1, DownloadState.DOWNLOADING)),
            waiting = emptyList(),
            current = 1,
          ),
        changes = emptyList(),
        startGalleryId = 1,
      )
    assertInvalid(active)

    val final = idlePlan().copy(changes = emptyList())
    val previous =
      DownloadQueueSnapshot(
        entries = listOf(DownloadEntry(1, DownloadState.FAILED)),
        waiting = emptyList(),
      )
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "downloadQueue", payload = final)
        .toDownloadQueuePlan(previous)
    }
  }

  private fun assertInvalid(plan: DownloadQueuePlan) {
    val error =
      assertFailsWith<IllegalArgumentException> {
        ApiEnvelope(version = 1, module = "downloadQueue", payload = plan).toDownloadQueuePlan()
      }
    assertTrue(error.message.orEmpty().isNotBlank())
  }

  private fun idlePlan() =
    DownloadQueuePlan(
      snapshot =
        DownloadQueueSnapshot(
          entries = listOf(DownloadEntry(1, DownloadState.NONE)),
          waiting = emptyList(),
        ),
      changes = emptyList(),
    )

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a36776657273696f6e0168736e617073686f74a367656e747269657381a263676964076573746174" +
        "656b646f776e6c6f6164696e676777616974696e67806763757272656e740767636f6d6d616e64" +
        "a1646b696e64687374617274416c6c"

    const val V1_RECOVERY_REQUEST_GOLDEN_CBOR_HEX =
      "a26776657273696f6e0167656e747269657381a263676964076573746174656b646f776e6c6f6164" + "696e67"

    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d646f776e6c6f61645175657565677061796c6f6164" +
        "a468736e617073686f74a367656e747269657381a263676964076573746174656b646f776e6c6f61" +
        "64696e676777616974696e67806763757272656e7407676368616e67657381a36367696407687072" +
        "6576696f7573646e6f6e656763757272656e746b646f776e6c6f6164696e67687374617274476964" +
        "076963616e63656c476964f6656572726f72f6"
  }
}
