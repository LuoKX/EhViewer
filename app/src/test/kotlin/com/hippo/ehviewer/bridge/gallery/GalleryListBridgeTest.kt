package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryItemSummary
import com.hippo.ehviewer.bridge.GalleryPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryListBridgeTest {
  @Test
  fun mapsEveryRustFieldToTheExistingAndroidModel() {
    val envelope =
      ApiEnvelope(
        version = 1,
        module = "galleryList",
        payload =
          GalleryPage(
            previous = "10-2",
            next = "20",
            items = listOf(completeItem()),
          ),
      )

    val result = envelope.toGalleryListResult()

    assertEquals("10-2", result.prev)
    assertEquals("20", result.next)
    assertEquals(1, result.galleryInfoList.size)
    with(result.galleryInfoList.single()) {
      assertEquals(42, gid)
      assertEquals("0123456789", token)
      assertEquals("Title", title)
      assertEquals("日本語", titleJpn)
      assertEquals("thumb.jpg", thumbKey)
      assertEquals(4, category)
      assertEquals("2026-08-02", posted)
      assertEquals("uploader", uploader)
      assertTrue(disowned)
      assertEquals(4.5f, rating)
      assertTrue(rated)
      assertEquals(listOf("tag"), simpleTags)
      assertEquals(12, pages)
      assertEquals(150, thumbWidth)
      assertEquals(200, thumbHeight)
      assertEquals("Japanese", simpleLanguage)
      assertEquals(0, favoriteSlot)
      assertEquals("Favorites", favoriteName)
      assertEquals("note", favoriteNote)
    }
  }

  @Test
  fun decodesTheRustGeneratedV1CborGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("galleryList", envelope.module)
    assertTrue(envelope.isOk())
    assertEquals("Title", envelope.toGalleryListResult().galleryInfoList.single().title)
  }

  @Test
  fun rejectsUnsupportedVersionAndUnexpectedModule() {
    val page = GalleryPage(items = emptyList())
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryList", payload = page).toGalleryListResult()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryDetail", payload = page).toGalleryListResult()
    }
  }

  @Test
  fun preservesTypedRustFailureForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.IP_BANNED, "blocked")
    val envelope = ApiEnvelope<GalleryPage>(version = 1, module = "galleryList", error = failure)

    val error = assertFailsWith<RustFailureException> { envelope.toGalleryListResult() }
    assertEquals(failure, error.failure)
  }

  @Test
  fun contractConstructorsRejectInvalidPayloadAndErrorStates() {
    assertFailsWith<IllegalArgumentException> {
      RustError(RustErrorKind.NO_HITS, "unexpected")
    }
    assertFailsWith<IllegalArgumentException> {
      RustError(RustErrorKind.GALLERY_UNAVAILABLE)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryPage>(version = 1, module = "galleryList")
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
        version = 1,
        module = "galleryList",
        payload = GalleryPage(items = emptyList()),
        error = RustError(RustErrorKind.NO_HITS),
      )
    }
  }

  private fun completeItem() =
    GalleryItemSummary(
      galleryId = 42,
      title = "Title",
      token = "0123456789",
      titleJapanese = "日本語",
      thumbnailKey = "thumb.jpg",
      category = 4,
      posted = "2026-08-02",
      uploader = "uploader",
      disowned = true,
      rating = 4.5f,
      rated = true,
      tags = listOf("tag"),
      pageCount = 12,
      thumbnailWidth = 150,
      thumbnailHeight = 200,
      language = "Japanese",
      favoriteSlot = 0,
      favoriteName = "Favorites",
      favoriteNote = "note",
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656b67616c6c6572794c697374677061796c6f6164a364" +
        "707265766431302d32646e6578746232306f67616c6c657279496e666f4c69737481b36367696418" +
        "2a657469746c65655469746c6565746f6b656e6a30313233343536373839687469746c654a706e69" +
        "e697a5e69cace8aa9e687468756d624b6579697468756d622e6a70676863617465676f7279046670" +
        "6f737465646a323032362d30382d30326875706c6f616465726875706c6f61646572686469736f77" +
        "6e6564f566726174696e67f94480657261746564f56a73696d706c65546167738163746167657061" +
        "6765730c6a7468756d62576964746818966b7468756d6248656967687418c86e73696d706c654c61" +
        "6e6775616765684a6170616e6573656c6661766f72697465536c6f74006c6661766f726974654e61" +
        "6d65694661766f72697465736c6661766f726974654e6f7465646e6f7465656572726f72f6"
  }
}
