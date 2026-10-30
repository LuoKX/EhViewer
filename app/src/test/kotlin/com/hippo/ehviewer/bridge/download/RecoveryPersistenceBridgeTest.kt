package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.*
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import java.io.File
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class RecoveryPersistenceBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  private fun gallery(title: String? = "incoming", rating: Float = 4.5f) =
    GalleryEntity(
      11,
      "fixture",
      title,
      title,
      title,
      2,
      title,
      title,
      rating,
      listOf("tag"),
      8,
      title,
      -2,
    )

  private fun row() =
    DownloadRecoveryCandidate(
      sourceKey = "source",
      rootKey = "root",
      sourceLocator = "fixture",
      relativePath = "11",
      displayName = "display",
      sourceKind = DownloadRecoverySourceKind.DIRECTORY,
      status = DownloadRecoveryStatus.READY,
      fingerprint = "fingerprint",
      lastSeenRunId = "run",
      createdAt = 100,
    )

  @Test
  fun formalMergeUsesJvmBlankFactsAndPreservesIdentityRatingBitsAndTags() {
    val incoming = gallery()
    assertSame(incoming, RustRecoveryMergePolicy.mergeGallery(null, incoming))
    for (value in listOf(null, "", " ", "\u00a0", "\u2007", "\u202f", "\u001c", "\uD800", "old")) {
      for (rating in listOf(0f, -0f, Float.fromBits(0x7fc00001), Float.NEGATIVE_INFINITY, 1f)) {
        val old =
          gallery(value, rating).apply {
            gid = 9
            token = "old"
            category = 0
            pages = -1
            simpleTags = emptyList()
            favoriteSlot = 6
          }
        val merged = RustRecoveryMergePolicy.mergeGallery(old, incoming)
        val expected = value.takeUnless { it.isNullOrBlank() } ?: incoming.title
        assertEquals(
          listOf(expected, expected, expected, expected, expected, expected),
          listOf(
            merged.title,
            merged.titleJpn,
            merged.thumbKey,
            merged.posted,
            merged.uploader,
            merged.simpleLanguage,
          ),
        )
        assertEquals(
          (if (rating == 0f) incoming.rating else rating).toRawBits(),
          merged.rating.toRawBits(),
        )
        assertEquals(9L, merged.gid)
        assertEquals("old", merged.token)
        assertEquals(6, merged.favoriteSlot)
        assertEquals(incoming.category, merged.category)
        assertEquals(incoming.pages, merged.pages)
        assertSame(incoming.simpleTags, merged.simpleTags)
      }
    }
    val old =
      gallery("old").apply {
        category = -2
        pages = 5
        favoriteSlot = -1
      }
    val merged = RustRecoveryMergePolicy.mergeGallery(old, incoming)
    assertEquals(-2, merged.category)
    assertEquals(5, merged.pages)
    assertSame(old.simpleTags, merged.simpleTags)
  }

  @Test
  fun artistTrimMatchesEveryJvmWhitespaceCodeUnitAndRetainsRawSurrogates() {
    val whitespace = (0..65535).map(Int::toChar).filter(Char::isWhitespace)
    val old = whitespace.map { "${it}artist${it}" } + listOf("\uD800", "\uDC00", "  ", "", "B")
    val incoming = listOf("artist", " B ", "new", "\uD800", "Ａ", "A")
    assertEquals(
      (old + incoming).map(String::trim).filter(String::isNotEmpty).distinct(),
      RustRecoveryMergePolicy.mergeArtists(old, incoming),
    )
    assertEquals(emptyList(), RustRecoveryMergePolicy.mergeArtists(listOf("　"), emptyList()))
  }

  @Test
  fun stagingRetainsEmptyIncomingValuesAndOldCreationFields() {
    val old =
      row()
        .copy(
          candidateId = 7,
          gid = 44,
          token = "old",
          pageToken = "oldPage",
          pageIndex = 3,
          title = "oldTitle",
          titleJpn = "旧",
          declaredPages = 20,
          createdAt = 10,
        )
    val incoming = row().copy(gid = 0, token = "", title = "", declaredPages = 0, updatedAt = 200)
    val request =
      RecoveryPersistenceRequest(
        kind = RecoveryPersistenceKind.STAGE,
        action = RecoveryPersistenceAction.PROMOTE_FINISHED,
        status = RecoveryStatus.READY,
        issues = listOf("MISSING_PAGE", "MISSING_PAGE"),
        pathAvailable = false,
        present = listOf(true, true, false, false, true, true),
      )
    val merged = applyRecoveryStage(incoming, old, planRecoveryPersistence(request))
    assertEquals(7L, merged.candidateId)
    assertEquals(10L, merged.createdAt)
    assertEquals(200L, merged.updatedAt)
    assertEquals(0L, merged.gid)
    assertEquals("", merged.token)
    assertEquals("", merged.title)
    assertEquals("旧", merged.titleJpn)
    assertEquals("oldPage", merged.pageToken)
    assertEquals(3, merged.pageIndex)
    assertEquals(0, merged.declaredPages)
    assertEquals(DownloadRecoveryStatus.PATH_CONFLICT, merged.status)
    assertEquals("MISSING_PAGE,ANDROID_PATH_CONFLICT", merged.issues)
    assertEquals(
      listOf(true),
      planRecoveryPersistence(request.copy(action = RecoveryPersistenceAction.KEEP_EXISTING)).flags,
    )
    for (issues in
      listOf(
        emptyList(),
        listOf("EXISTING_MAPPING_CONFLICT"),
        listOf("DUPLICATE_SOURCE", "EXISTING_MAPPING_CONFLICT"),
      )) {
      val p =
        planRecoveryPersistence(
          request.copy(
            action = RecoveryPersistenceAction.STAGE,
            status = RecoveryStatus.CONFLICT,
            issues = issues,
          )
        )
      assertEquals(
        when {
          "DUPLICATE_SOURCE" in issues -> "DUPLICATE_SOURCE"
          "EXISTING_MAPPING_CONFLICT" in issues -> "PATH_CONFLICT"
          else -> "IDENTITY_CONFLICT"
        },
        p.status,
      )
    }
  }

  @Test
  fun failureIssueFragmentsAreFilteredWithoutTrimmingAndFinishRequiresAnExhaustiveScan() {
    val original = row().copy(issues = " ,A, A ,A,\u00a0,\uD800,PROMOTION_CONFLICT")
    var ticks = 0
    val failed =
      recoveryPromotionFailure(original) {
        ticks++
        500
      }
    assertEquals(
      (original.issues.split(',').filter(String::isNotBlank) + "PROMOTION_CONFLICT")
        .distinct()
        .joinToString(","),
      failed.issues,
    )
    assertEquals(1, ticks)
    assertEquals(500L, failed.updatedAt)
    for (bits in 0..7) assertEquals(
      listOf(bits != 0, bits == 0),
      planRecoveryPersistence(
          RecoveryPersistenceRequest(
            kind = RecoveryPersistenceKind.FINISH,
            limits = (0..2).map { bits and (1 shl it) != 0 },
          )
        )
        .flags,
    )
    assertEquals(
      1,
      planRecoveryPersistence(
          RecoveryPersistenceRequest(
            kind = RecoveryPersistenceKind.PENDING,
            sameRoot = listOf(true, true, false, false),
            missing = listOf(false, true, false, true),
          )
        )
        .count,
    )
  }

  @Test
  fun exactWireGoldensCrossTheRealJniBoundary() {
    val artists =
      RecoveryPersistenceRequest(
        kind = RecoveryPersistenceKind.ARTISTS,
        names =
          listOf(" A ", "A", "\uD800", "　B　", "").map { RecoveryRawText(it.toUtf16LeBytes()) },
        whitespace = listOf(32, 12288),
      )
    val stage =
      RecoveryPersistenceRequest(
        kind = RecoveryPersistenceKind.STAGE,
        action = RecoveryPersistenceAction.PROMOTE_FINISHED,
        status = RecoveryStatus.READY,
        issues = listOf("MISSING_PAGE", "MISSING_PAGE"),
        pathAvailable = false,
        present = listOf(true, false, true, false, true, false),
      )
    for ((name, request) in listOf("artists" to artists, "stage" to stage)) {
      val root = File(UpdaterHostLibrary.root, "app/src/main/rust/crates/domain/test_data")
      fun golden(suffix: String) =
        File(root, "recovery_${name}_${suffix}.cbor.hex")
          .readText()
          .trim()
          .chunked(2)
          .map { it.toInt(16).toByte() }
          .toByteArray()
      assertContentEquals(golden("request"), rustCbor.encodeToByteArray(request))
      planRecoveryPersistence(request) { buffer, size ->
        nativeRecoveryPersistenceV1(buffer, size).also {
          assertContentEquals(golden("response"), it)
        }
      }
    }
  }

  @Test
  fun malformedPlansAndRequestsRejectBeforeAssignment() {
    val r =
      RecoveryPersistenceRequest(
        kind = RecoveryPersistenceKind.ARTISTS,
        names = listOf(RecoveryRawText("A".toUtf16LeBytes())),
        whitespace = emptyList(),
      )
    for (p in
      listOf(
        RecoveryPersistencePlan(emptyList(), listOf(listOf(0, 0, 2)), null, null, null),
        RecoveryPersistencePlan(listOf(true), emptyList(), null, null, null),
        RecoveryPersistencePlan(emptyList(), emptyList(), byteArrayOf(), "READY", null),
        RecoveryPersistencePlan(
          emptyList(),
          listOf(listOf(0, 0, 1), listOf(0, 0, 1)),
          null,
          null,
          null,
        ),
      )) assertFails {
      planRecoveryPersistence(r) { _, _ ->
        rustCbor.encodeToByteArray(ApiEnvelope(1, "recoveryPersistence", p, null))
      }
    }
    assertFails { planRecoveryPersistence(r.copy(version = 2)) }
    assertFails { planRecoveryPersistence(r.copy(whitespace = listOf(32, 32))) }
    assertFails { planRecoveryPersistence(r.copy(names = listOf(RecoveryRawText(byteArrayOf(1))))) }
    assertFails { planRecoveryPersistence(r.copy(category = 1)) }
  }
}
