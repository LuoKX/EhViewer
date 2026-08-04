package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.V1GalleryPreview
import com.ehviewer.core.model.V2GalleryPreview
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryPreview
import com.hippo.ehviewer.bridge.GalleryPreviewKind
import com.hippo.ehviewer.bridge.GalleryPreviewPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryPreviewBridgeTest {
  @Test
  fun mapsStandaloneAndSpriteContractsToExistingAndroidModels() {
    val result = successfulEnvelope().toGalleryPreviewList()

    assertEquals(12, result.total)
    assertEquals(2, result.previews.size)
    with(assertIs<V1GalleryPreview>(result.previews[0])) {
      assertEquals("https://img.example/one.jpg", url)
      assertEquals(0, position)
      assertEquals("abcdef1234", pToken)
    }
    with(assertIs<V2GalleryPreview>(result.previews[1])) {
      assertEquals("https://img.example/sprite.jpg", url)
      assertEquals(1, position)
      assertEquals("", pToken)
      assertEquals(16, offsetX)
      assertEquals(80, clipWidth)
      assertEquals(120, clipHeight)
    }
  }

  @Test
  fun decodesTheRustGeneratedV1CborGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryPreviewPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("galleryPreview", envelope.module)
    assertTrue(envelope.isOk())
    assertEquals(12, envelope.toGalleryPreviewList().total)
  }

  @Test
  fun rejectsUnsupportedVersionAndUnexpectedModule() {
    val page = GalleryPreviewPage(previews = emptyList(), total = 1)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryPreview", payload = page).toGalleryPreviewList()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryDetail", payload = page).toGalleryPreviewList()
    }
  }

  @Test
  fun preservesTypedRustFailureForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.GALLERY_UNAVAILABLE, "removed")
    val envelope =
      ApiEnvelope<GalleryPreviewPage>(
        version = 1,
        module = "galleryPreview",
        error = failure,
      )

    val error = assertFailsWith<RustFailureException> { envelope.toGalleryPreviewList() }
    assertEquals(failure, error.failure)
  }

  @Test
  fun rejectsInvalidPreviewGeometryAndPageBoundaries() {
    val invalidStandalone =
      GalleryPreview(
        kind = GalleryPreviewKind.V1,
        url = "https://img.example/one.jpg",
        position = 0,
        pageToken = "token",
        offsetX = 1,
      )
    assertFailsWith<IllegalArgumentException> {
      previewEnvelope(invalidStandalone).toGalleryPreviewList()
    }

    val incompleteSprite =
      GalleryPreview(
        kind = GalleryPreviewKind.V2,
        url = "https://img.example/sprite.jpg",
        position = 0,
        pageToken = "token",
        offsetX = 0,
        clipWidth = 80,
      )
    assertFailsWith<IllegalStateException> {
      previewEnvelope(incompleteSprite).toGalleryPreviewList()
    }

    val negativePosition = standalonePreview().copy(position = -1)
    assertFailsWith<IllegalArgumentException> {
      previewEnvelope(negativePosition).toGalleryPreviewList()
    }
    val outOfRangePosition = standalonePreview().copy(position = 1)
    assertFailsWith<IllegalArgumentException> {
      previewEnvelope(outOfRangePosition).toGalleryPreviewList()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = GalleryPreviewPage(emptyList(), 0)).toGalleryPreviewList()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = GalleryPreviewPage(emptyList(), 1)).toGalleryPreviewList()
    }
  }

  private fun successfulEnvelope() =
    ApiEnvelope(
      version = 1,
      module = "galleryPreview",
      payload =
        GalleryPreviewPage(
          previews =
            listOf(
              standalonePreview(),
              GalleryPreview(
                kind = GalleryPreviewKind.V2,
                url = "https://img.example/sprite.jpg",
                position = 1,
                pageToken = "",
                offsetX = 16,
                clipWidth = 80,
                clipHeight = 120,
              ),
            ),
          total = 12,
        ),
    )

  private fun previewEnvelope(preview: GalleryPreview) =
    ApiEnvelope(
      version = 1,
      module = "galleryPreview",
      payload = GalleryPreviewPage(listOf(preview), 1),
    )

  private fun standalonePreview() =
    GalleryPreview(
      kind = GalleryPreviewKind.V1,
      url = "https://img.example/one.jpg",
      position = 0,
      pageToken = "abcdef1234",
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e67616c6c65727950726576696577677061796c6f61" +
        "64a268707265766965777382a7646b696e646276316375726c781b68747470733a2f2f696d672e65" +
        "78616d706c652f6f6e652e6a706768706f736974696f6e006670546f6b656e6a61626364656631" +
        "323334676f666673657458f669636c69705769647468f66a636c6970486569676874f6a7646b696e" +
        "646276326375726c781e68747470733a2f2f696d672e6578616d706c652f7370726974652e6a7067" +
        "68706f736974696f6e016670546f6b656e60676f6666736574581069636c6970576964746818506a" +
        "636c6970486569676874187865746f74616c0c656572726f72f6"
  }
}
