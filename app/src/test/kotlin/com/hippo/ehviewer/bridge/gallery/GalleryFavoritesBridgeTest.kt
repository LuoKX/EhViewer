package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryFavoritesPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryFavoritesBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixture() {
    val result =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryFavoritesPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())
        .toFavoritesResult()

    assertEquals(
      listOf(
        "Favorites 0",
        "Favorites & One",
        "Favorites × Two",
        "Favorites 3",
        "Favorites 4",
        "Favorites 5",
        "Favorites 6",
        "Favorites 7",
        "Favorites 8",
        "Favorites 9",
      ),
      result.catArray,
    )
    assertEquals((0..9).toList(), result.countArray)
    assertEquals("10-2", result.prev)
    assertEquals("20", result.next)
    assertEquals(2, result.galleryInfoList.size)
    assertEquals(123_456L, result.galleryInfoList[0].gid)
    assertEquals("Read later", result.galleryInfoList[0].favoriteName)
    assertEquals("Keep × 2", result.galleryInfoList[0].favoriteNote)
  }

  @Test
  fun rejectsVersionModuleAndCategoryPairingDrift() {
    val envelope = goldenEnvelope()
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(version = 2).toFavoritesResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(module = "galleryFavoritesUrl").toFavoritesResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope
        .copy(payload = envelope.payload?.copy(categories = listOf("one")))
        .toFavoritesResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(payload = envelope.payload?.copy(counts = listOf(1))).toFavoritesResult()
    }
  }

  @Test
  fun preservesTypedRustFailuresForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "Gallery favorites response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryFavoritesPage>(
            version = 1,
            module = "galleryFavorites",
            error = failure,
          )
          .toFavoritesResult()
      }

    assertEquals(failure, exception.failure)
  }

  private fun goldenEnvelope() =
    Cbor.decodeFromByteArray<ApiEnvelope<GalleryFavoritesPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657067616c6c6572794661766f7269746573677061796c6f6164a36a63617465" +
        "676f726965738a6b4661766f726974657320306f4661766f72697465732026204f6e65704661766f726974657320c397" +
        "2054776f6b4661766f726974657320336b4661766f726974657320346b4661766f726974657320356b4661766f726974" +
        "657320366b4661766f726974657320376b4661766f726974657320386b4661766f7269746573203966636f756e74738a" +
        "000102030405060708096470616765a364707265766431302d32646e6578746232306f67616c6c657279496e666f4c69" +
        "737482b3636769641a0001e240657469746c656c5469746c652026204d6f726565746f6b656e6a616263646566313233" +
        "34687469746c654a706ef6687468756d624b65796e73616d706c652f6f6e652e6a70676863617465676f72790466706f" +
        "7374656470323032362d30382d30322031323a30306875706c6f616465726b416c696365202620426f62686469736f77" +
        "6e6564f466726174696e67f94100657261746564f56a73696d706c655461677382706c616e67756167653a656e676c69" +
        "73686d6172746973743a73616d706c656570616765730c6a7468756d62576964746818966b7468756d62486569676874" +
        "18c86e73696d706c654c616e6775616765f66c6661766f72697465536c6f74006c6661766f726974654e616d656a5265" +
        "6164206c617465726c6661766f726974654e6f7465694b65657020c3972032b3636769641a0009fbf1657469746c6567" +
        "4d696e696d616c65746f6b656e6a30313233343536373839687469746c654a706ef6687468756d624b65796e73616d70" +
        "6c652f74776f2e6a70676863617465676f727919080066706f73746564606875706c6f616465726a284469736f776e65" +
        "6429686469736f776e6564f566726174696e67f9bc00657261746564f46a73696d706c65546167738065706167657300" +
        "6a7468756d625769647468006b7468756d62486569676874006e73696d706c654c616e6775616765f66c6661766f7269" +
        "7465536c6f74216c6661766f726974654e616d65f66c6661766f726974654e6f7465f6656572726f72f6"
  }
}
