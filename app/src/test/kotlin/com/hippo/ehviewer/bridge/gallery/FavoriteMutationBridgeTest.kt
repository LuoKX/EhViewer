package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class FavoriteMutationBridgeTest {
  private val request =
    FavoriteMutationRequest(
      1,
      GalleryUrlSite.PUBLIC,
      single = null,
      bulk = FavoriteMutationBulk(listOf(-1, 2), 3, -1),
    )
  private val expected =
    FavoriteMutationPlan(
      FavoriteMutationKind.BULK,
      "https://e-hentai.org/favorites.php?favcat=3",
      "https://e-hentai.org/favorites.php?favcat=3",
      "https://e-hentai.org",
      listOf(
        FavoriteMutationField("ddact", "delete"),
        FavoriteMutationField("modifygids[]", "-1"),
        FavoriteMutationField("modifygids[]", "2"),
      ),
    )

  @Test
  fun requestAndResponseGoldensRoundTrip() {
    assertEquals(REQUEST_GOLDEN, rustCbor.encodeToByteArray(request).toHex())
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<FavoriteMutationPlan>>(RESPONSE_GOLDEN.decodeHex())
    assertEquals(expected, envelope.toFavoriteMutationPlan(expected.kind))
  }

  @Test
  fun nativeCallbackReceivesCompleteRequestOnce() {
    var calls = 0
    assertEquals(
      expected,
      planFavoriteMutation(request) { buffer, size, output ->
        calls++
        val raw = ByteArray(size).also { buffer.get(it) }
        assertEquals(REQUEST_GOLDEN, raw.toHex())
        assertEquals(request, rustCbor.decodeFromByteArray<FavoriteMutationRequest>(raw))
        val response = RESPONSE_GOLDEN.decodeHex()
        output.put(response)
        response.size
      },
    )
    assertEquals(1, calls)
  }

  @Test
  fun validatorRejectsForgedEnvelopes() {
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(2, FAVORITE_MUTATION_MODULE, expected).toFavoriteMutationPlan(expected.kind)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(1, "other", expected).toFavoriteMutationPlan(expected.kind)
    }
    assertFailsWith<com.hippo.ehviewer.bridge.RustFailureException> {
      ApiEnvelope<FavoriteMutationPlan>(
          1,
          FAVORITE_MUTATION_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toFavoriteMutationPlan(expected.kind)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<FavoriteMutationPlan>(1, FAVORITE_MUTATION_MODULE)
        .toFavoriteMutationPlan(expected.kind)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(1, FAVORITE_MUTATION_MODULE, expected.copy(kind = FavoriteMutationKind.SINGLE))
        .toFavoriteMutationPlan(expected.kind)
    }
  }

  @Test
  fun constructorRejectsNeitherAndBothPayloads() {
    assertFailsWith<IllegalArgumentException> {
      FavoriteMutationRequest(1, GalleryUrlSite.PUBLIC, single = null, bulk = null)
    }
    assertFailsWith<IllegalArgumentException> {
      FavoriteMutationRequest(
        1,
        GalleryUrlSite.PUBLIC,
        FavoriteMutationSingle(1, "x", 0, ""),
        FavoriteMutationBulk(emptyList(), 0, -1),
      )
    }
  }

  private fun ByteArray.toHex() = joinToString("") { it.toUByte().toString(16).padStart(2, '0') }

  private companion object {
    const val REQUEST_GOLDEN =
      "a36776657273696f6e016473697465667075626c69636462756c6ba36369647382200266736f75726365036b64657374696e6174696f6e20"
    const val RESPONSE_GOLDEN =
      "a46776657273696f6e01666d6f64756c657767616c6c6572794661766f726974654d75746174696f6e677061796c6f6164a5646b696e646462756c6b6375726c782b68747470733a2f2f652d68656e7461692e6f72672f6661766f72697465732e7068703f6661766361743d336772656665726572782b68747470733a2f2f652d68656e7461692e6f72672f6661766f72697465732e7068703f6661766361743d33666f726967696e7468747470733a2f2f652d68656e7461692e6f726764666f726d83a2646e616d656564646163746576616c75656664656c657465a2646e616d656c6d6f64696679676964735b5d6576616c7565622d31a2646e616d656c6d6f64696679676964735b5d6576616c75656132656572726f72f6"
  }
}
