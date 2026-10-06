package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

// Rust encodes explicit nulls. Use scalar null serializers: definite-length CBOR's
// nullable-object encoder emits an empty map, unlike its nullable-scalar encoder.
@Serializable
private data class DownloadMetadataGoldenEnvelope<P, E>(
  val version: Int,
  val module: String,
  val payload: P,
  val error: E,
)

class DownloadMetadataBridgeTest {
  @Test
  fun sharedCborGoldensEncodeIndependentlyAndInvokeOnce() {
    val entries =
      listOf(
        DownloadMetadataSnapshot(3, false, false, true),
        DownloadMetadataSnapshot(0, false, false, true),
        DownloadMetadataSnapshot(9, true, true, true),
        DownloadMetadataSnapshot(4, false, true, true),
        DownloadMetadataSnapshot(0, true, true, false),
      )
    val observe = DownloadMetadataRequest(1, DownloadMetadataPhase.OBSERVE, entries, emptyList())
    val patch =
      observe.copy(
        phase = DownloadMetadataPhase.PATCH,
        observations =
          listOf(
            DownloadMetadataObservation(1, null, 6),
            DownloadMetadataObservation(
              2,
              DownloadMetadataComicFacts(-7, listOf("group:g", "artist:a"), listOf("a", "a")),
              null,
            ),
            DownloadMetadataObservation(
              3,
              DownloadMetadataComicFacts(-7, listOf("group:g", "artist:a"), null),
              null,
            ),
          ),
      )
    val noop =
      patch.copy(
        observations =
          patch.observations.map {
            it.copy(comic = null, spiderPages = null)
          }
      )
    val expectedReads =
      listOf(
        DownloadMetadataRead(1, true),
        DownloadMetadataRead(2, false),
        DownloadMetadataRead(3, false),
      )
    val expectedPatches =
      listOf(
        DownloadMetadataPatch(1, 6, false, null, null, true, false),
        DownloadMetadataPatch(
          2,
          -7,
          true,
          listOf("group:g", "artist:a"),
          listOf("a", "a"),
          true,
          true,
        ),
      )
    for ((name, request, expected) in
      listOf(
        Triple("observe", observe, DownloadMetadataPlan(expectedReads, emptyList())),
        Triple("patch", patch, DownloadMetadataPlan(emptyList(), expectedPatches)),
        Triple("noop", noop, DownloadMetadataPlan(emptyList(), emptyList())),
      )) {
      assertContentEquals(golden("${name}_request"), rustCbor.encodeToByteArray(request))
      assertContentEquals(
        golden("${name}_response"),
        rustCbor.encodeToByteArray(
          DownloadMetadataGoldenEnvelope<DownloadMetadataPlan, String?>(
            1,
            "downloadMetadata",
            expected,
            null,
          )
        ),
      )
      assertEquals(
        request,
        rustCbor.decodeFromByteArray<DownloadMetadataRequest>(golden("${name}_request")),
      )
      var calls = 0
      val actual =
        planDownloadMetadata(request) { input, size, output ->
          calls++
          assertContentEquals(
            golden("${name}_request"),
            ByteArray(size).also { input.duplicate().get(it) },
          )
          respond(output, golden("${name}_response"))
        }
      assertEquals(expected, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun invalidRequestVersionRowsSourceSlotsAndConflictingFactsNeverEnterJni() {
    val observe = observe()
    val patch = patch()
    val fact = patch.observations.single()
    for (wrong in
      listOf(
        observe.copy(version = 2),
        observe.copy(entries = List(MAX_DOWNLOAD_METADATA_ROWS + 1) { observe.entries.single() }),
        observe.copy(observations = patch.observations),
        patch.copy(observations = listOf(fact.copy(index = -1))),
        patch.copy(observations = listOf(fact.copy(index = 1))),
        patch.copy(observations = listOf(fact, fact)),
        patch.copy(observations = listOf(fact.copy(spiderPages = 1))),
        patch.copy(entries = listOf(patch.entries.single().copy(hasDirectory = false))),
        patch.copy(
          entries = List(2) { patch.entries.single() },
          observations = listOf(fact.copy(index = 1), fact),
        ),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(wrong) { _, _, _ -> fail("Invalid request entered JNI") }
      }
    }
  }

  @Test
  fun stringAndUtf8BudgetsRejectBeforeJniWithoutPayloadInError() {
    for (strings in
      listOf(
        List(MAX_DOWNLOAD_METADATA_STRINGS + 1) { "secret" },
        listOf("é".repeat(MAX_DOWNLOAD_METADATA_TEXT_BYTES / 2 + 1)),
      )) {
      val request =
        patch()
          .copy(
            observations =
              listOf(
                DownloadMetadataObservation(0, DownloadMetadataComicFacts(0, strings, null), null)
              )
          )
      val failure =
        assertFailsWith<IllegalArgumentException> {
          planDownloadMetadata(request) { _, _, _ -> fail("Oversize request entered JNI") }
        }
      assertEquals("Download metadata exceeds resource budget", failure.message)
    }
  }

  @Test
  fun envelopeAndResponseSizesRejectWithoutFallback() {
    for (envelope in
      listOf(
        ApiEnvelope(2, "downloadMetadata", reads()),
        ApiEnvelope(1, "comicInfo", reads()),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(
          observe(),
          native(envelope),
        )
      }
    }
    for (invalid in
      listOf(
        rustCbor.encodeToByteArray(
          DownloadMetadataGoldenEnvelope<String?, String?>(1, "downloadMetadata", null, null)
        ),
        rustCbor.encodeToByteArray(
          DownloadMetadataGoldenEnvelope(
            1,
            "downloadMetadata",
            reads(),
            RustError(RustErrorKind.ERROR, "fixture"),
          )
        ),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(observe()) { _, _, output ->
          respond(output, invalid)
        }
      }
    }
    for (size in listOf(0, -1, MAX_DOWNLOAD_METADATA_WIRE_BYTES + 1)) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(observe()) { _, _, _ -> size }
      }
    }
  }

  @Test
  fun readPlanRejectsDuplicateOutOfRangeReorderedOrUnavailableSourceSlots() {
    val request = observe().copy(entries = List(2) { observe().entries.single() })
    val valid = listOf(DownloadMetadataRead(0, true), DownloadMetadataRead(1, true))
    for (reads in
      listOf(
        valid.reversed(),
        valid + valid[0],
        valid.map {
          it.copy(index = 2)
        },
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(request, success(DownloadMetadataPlan(reads, emptyList())))
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(
        observe().copy(entries = listOf(observe().entries.single().copy(hasDirectory = false))),
        success(reads()),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(observe(), success(reads().copy(patches = listOf(validPatch()))))
    }
  }

  @Test
  fun patchPlanRejectsMalformedSlotsProvenancePersistenceAndNoopShape() {
    val good = validPatch()
    for (patches in
      listOf(
        listOf(good, good),
        listOf(good.copy(index = -1)),
        listOf(good.copy(index = 1)),
        listOf(good.copy(pages = 8)),
        listOf(good.copy(simpleTags = emptyList())),
        listOf(good.copy(artists = listOf("other"))),
        listOf(good.copy(persistGallery = false)),
        listOf(good.copy(persistArtists = false)),
        listOf(
          good.copy(
            pages = null,
            updateTags = false,
            artists = null,
            persistGallery = false,
            persistArtists = false,
          )
        ),
        listOf(good.copy(updateTags = false, simpleTags = listOf("unapplied"))),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(patch(), success(DownloadMetadataPlan(emptyList(), patches)))
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(patch(), success(DownloadMetadataPlan(reads().reads, listOf(good))))
    }
    val unobserved = patch().copy(entries = List(2) { patch().entries.single() })
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(
        unobserved,
        success(DownloadMetadataPlan(emptyList(), listOf(good.copy(index = 1)))),
      )
    }
    val fallback = patch().copy(observations = listOf(DownloadMetadataObservation(0, null, -1)))
    val fallbackPatch =
      good.copy(
        pages = -1,
        updateTags = false,
        simpleTags = null,
        artists = null,
        persistArtists = false,
      )
    assertEquals(
      listOf(fallbackPatch),
      planDownloadMetadata(
          fallback,
          success(DownloadMetadataPlan(emptyList(), listOf(fallbackPatch))),
        )
        .patches,
    )
    for (wrong in
      listOf(
        fallbackPatch.copy(updateTags = true),
        fallbackPatch.copy(artists = emptyList()),
        fallbackPatch.copy(persistGallery = false),
        fallbackPatch.copy(pages = 1),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planDownloadMetadata(fallback, success(DownloadMetadataPlan(emptyList(), listOf(wrong))))
      }
    }
  }

  @Test
  fun artistOnlyCannotOverwriteGalleryAndEmptyArtistListStillPersists() {
    val request =
      patch()
        .copy(
          entries = listOf(DownloadMetadataSnapshot(5, false, true, true)),
          observations =
            listOf(
              DownloadMetadataObservation(
                0,
                DownloadMetadataComicFacts(99, null, emptyList()),
                null,
              )
            ),
        )
    val allowed = DownloadMetadataPatch(0, null, false, null, emptyList(), false, true)
    assertEquals(
      listOf(allowed),
      planDownloadMetadata(
          request,
          success(DownloadMetadataPlan(emptyList(), listOf(allowed))),
        )
        .patches,
    )
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(
        request,
        success(
          DownloadMetadataPlan(emptyList(), listOf(allowed.copy(pages = 99, persistGallery = true)))
        ),
      )
    }
  }

  @Test
  fun nativeSelectionPriorityAndPartialWriteChoicesAreNotRecomputed() {
    val request = observe().copy(entries = List(2) { observe().entries.single() })
    for (plan in
      listOf(
        DownloadMetadataPlan(emptyList(), emptyList()),
        DownloadMetadataPlan(listOf(DownloadMetadataRead(1, false)), emptyList()),
      )) assertEquals(plan, planDownloadMetadata(request, success(plan)))
    val complete = observe().copy(entries = listOf(DownloadMetadataSnapshot(5, false, false, true)))
    assertEquals(reads(), planDownloadMetadata(complete, success(reads())))
    val good = validPatch()
    for (patches in
      listOf(
        emptyList(),
        listOf(good.copy(pages = null)),
        listOf(good.copy(updateTags = false)),
        listOf(good.copy(pages = null, updateTags = false, persistGallery = false)),
        listOf(good.copy(artists = null, persistArtists = false)),
      )) {
      val plan = DownloadMetadataPlan(emptyList(), patches)
      assertEquals(plan, planDownloadMetadata(patch(), success(plan)))
    }
  }

  @Test
  fun rustValidatesObservationCompletenessAndFallbackPriorityWithoutKotlinOracle() {
    val malformedPolicyRequests =
      listOf(
        patch().copy(observations = emptyList()),
        patch()
          .copy(
            entries = listOf(patch().entries.single().copy(pages = 9)),
            observations = listOf(DownloadMetadataObservation(0, null, 1)),
          ),
      )
    for (request in malformedPolicyRequests) {
      var calls = 0
      val rejected =
        ApiEnvelope<DownloadMetadataPlan>(
          1,
          "downloadMetadata",
          error = RustError(RustErrorKind.ERROR, "Invalid download metadata input"),
        )
      assertFailsWith<RustFailureException> {
        planDownloadMetadata(request) { _, _, output ->
          calls++
          respond(output, rustCbor.encodeToByteArray(rejected))
        }
      }
      assertEquals(1, calls)
    }
  }

  @Test
  fun completeFieldsCannotBeWrittenDespiteValidSidecarProvenance() {
    val request = patch().copy(entries = listOf(DownloadMetadataSnapshot(5, false, false, true)))
    for (selected in
      listOf(
        validPatch(),
        validPatch().copy(artists = null, persistArtists = false),
        validPatch().copy(pages = null, updateTags = false, persistGallery = false),
      )) assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(request, success(DownloadMetadataPlan(emptyList(), listOf(selected))))
    }
    val existingArtists =
      request.copy(entries = listOf(DownloadMetadataSnapshot(0, true, false, true)))
    assertFailsWith<IllegalArgumentException> {
      planDownloadMetadata(
        existingArtists,
        success(DownloadMetadataPlan(emptyList(), listOf(validPatch()))),
      )
    }
  }

  @Test
  fun nativeTypedLinkageCancellationAndMalformedCborFailuresSurfaceOnce() {
    assertFailsWith<RustFailureException> {
      planDownloadMetadata(
        observe(),
        native(
          ApiEnvelope(
            1,
            "downloadMetadata",
            error = RustError(RustErrorKind.ERROR, "Invalid download metadata input"),
          )
        ),
      )
    }
    for (failure in
      listOf(
        LinkageError("fixture"),
        IllegalStateException("fixture"),
        CancellationException("fixture"),
      )) {
      var calls = 0
      try {
        planDownloadMetadata(observe()) { _, _, _ ->
          calls++
          throw failure
        }
        fail("Native failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(1, calls)
    }
    assertFails {
      planDownloadMetadata(observe()) { _, _, output ->
        respond(output, byteArrayOf(0xff.toByte()))
      }
    }
    assertFails {
      rustCbor.decodeFromByteArray<DownloadMetadataRequest>(byteArrayOf(0xff.toByte()))
    }
  }

  private fun observe() =
    DownloadMetadataRequest(
      1,
      DownloadMetadataPhase.OBSERVE,
      listOf(DownloadMetadataSnapshot(0, true, true, true)),
      emptyList(),
    )

  private fun patch() =
    observe()
      .copy(
        phase = DownloadMetadataPhase.PATCH,
        observations =
          listOf(
            DownloadMetadataObservation(
              0,
              DownloadMetadataComicFacts(7, null, listOf("a", "a")),
              null,
            )
          ),
      )

  private fun reads() = DownloadMetadataPlan(listOf(DownloadMetadataRead(0, true)), emptyList())

  private fun validPatch() = DownloadMetadataPatch(0, 7, true, null, listOf("a", "a"), true, true)

  private fun success(plan: DownloadMetadataPlan) = native(ApiEnvelope(1, "downloadMetadata", plan))

  private fun native(envelope: ApiEnvelope<DownloadMetadataPlan>): DownloadMetadataNativeCall =
    { _, _, output ->
      respond(output, rustCbor.encodeToByteArray(envelope))
    }

  private fun respond(
    output: ByteBuffer,
    bytes: ByteArray,
  ): Int {
    output.put(bytes)
    return bytes.size
  }

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/download_metadata_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map {
        it.toInt(16).toByte()
      }
      .toByteArray()
}
