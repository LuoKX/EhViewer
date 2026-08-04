package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class FavoritesUrlBridgeTest {
  @Test
  fun encodesTheRustGeneratedRequestGoldenFixture() {
    val encodedHex =
      rustCbor.encodeToByteArray(goldenRequest()).joinToString(separator = "") {
        it.toUByte().toString(16).padStart(2, '0')
      }

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encodedHex)
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedResponseGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryUrlPlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )

    assertEquals(EXPECTED_URL, envelope.toFavoritesUrl())
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithTheCompleteSanitizedState() {
    val request = goldenRequest()
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(
          version = 1,
          module = "galleryFavoritesUrl",
          payload = GalleryUrlPlan(EXPECTED_URL),
        )
      )
    var invocationCount = 0

    val actual =
      buildFavoritesUrl(request) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        assertEquals(request, rustCbor.decodeFromByteArray<FavoritesUrlRequest>(requestBytes))
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(EXPECTED_URL, actual)
  }

  @Test
  fun rejectsWrongEnvelopeTypedFailureMissingPayloadAndBlankUrl() {
    val plan = GalleryUrlPlan(EXPECTED_URL)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryFavoritesUrl", payload = plan).toFavoritesUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryListUrl", payload = plan).toFavoritesUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryUrlPlan>(version = 1, module = "galleryFavoritesUrl").toFavoritesUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryFavoritesUrl", payload = GalleryUrlPlan(""))
        .toFavoritesUrl()
    }

    val failure = RustError(RustErrorKind.ERROR, "unsupported favorites URL protocol version")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryUrlPlan>(
            version = 1,
            module = "galleryFavoritesUrl",
            error = failure,
          )
          .toFavoritesUrl()
      }
    assertEquals(failure, error.failure)
  }

  private fun goldenRequest() =
    FavoritesUrlRequest(
      version = 1,
      site = GalleryUrlSite.EX_HENTAI,
      favoriteCategory = 4,
      keyword = "translated",
      previous = null,
      next = "20",
      seek = null,
    )

  private companion object {
    const val EXPECTED_URL =
      "https://exhentai.org/favorites.php?favcat=4&f_search=translated&next=20"
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a76776657273696f6e01647369746568657848656e746169706661766f726974654361746567" +
        "6f727904676b6579776f72646a7472616e736c617465646870726576696f7573f6646e657874" +
        "623230647365656bf6"
    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974657355726c67" +
        "7061796c6f6164a16375726c784768747470733a2f2f657868656e7461692e6f72672f666176" +
        "6f72697465732e7068703f6661766361743d3426665f7365617263683d7472616e736c617465" +
        "64266e6578743d3230656572726f72f6"
  }
}
