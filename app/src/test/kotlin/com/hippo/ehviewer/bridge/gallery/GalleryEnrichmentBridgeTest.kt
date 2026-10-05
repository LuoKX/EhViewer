package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.encodeToByteArray

class GalleryEnrichmentBridgeTest {
  private fun fixture(name: String) =
    File("src/main/rust/crates/domain/test_data/gallery_enrichment_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  @Test
  fun exactRustGoldensAndSingleInvocationPinLanguagePriority() {
    val request = GalleryEnrichmentRequest(1, GalleryEnrichmentKind.LANGUAGE, title = "[ENG]")
    assertEquals(fixture("request").toList(), rustCbor.encodeToByteArray(request).toList())
    var calls = 0
    val result =
      planGalleryEnrichment(request) { _, _, output ->
        calls++
        val bytes = fixture("response")
        output.put(bytes)
        bytes.size
      }
    assertEquals("EN", result.language)
    assertEquals(1, calls)
    val info = BaseGalleryInfo(gid = 1, token = "abc", title = "[ENG]")
    info.generateSLang { title, tags ->
      assertEquals("[ENG]", title)
      assertEquals(null, tags)
      result.language
    }
    assertEquals("EN", info.simpleLanguage)
  }

  @Test
  fun malformedKindLanguageAndEligibilityResultsFailClosed() {
    val request = GalleryEnrichmentRequest(1, GalleryEnrichmentKind.LANGUAGE)
    for (plan in
      listOf(
        GalleryEnrichmentPlan(GalleryEnrichmentKind.METADATA_NEEDED, null, true),
        GalleryEnrichmentPlan(GalleryEnrichmentKind.LANGUAGE, "unknown", null),
        GalleryEnrichmentPlan(GalleryEnrichmentKind.LANGUAGE, "EN", true),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryEnrichment(request) { _, _, output ->
          val bytes = rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryEnrichment", plan))
          output.put(bytes)
          bytes.size
        }
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      planGalleryEnrichment(request) { _, _, _ -> throw UnsatisfiedLinkError("JNI") }
    }
  }
}
