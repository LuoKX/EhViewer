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

class GalleryListUrlBridgeTest {
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

    assertEquals(EXPECTED_URL, envelope.toGalleryListUrl())
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithTheCompleteSanitizedState() {
    val request = goldenRequest()
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(
          version = 1,
          module = "galleryListUrl",
          payload = GalleryUrlPlan(EXPECTED_URL),
        )
      )
    var invocationCount = 0

    val actual =
      buildGalleryListUrl(request) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        assertEquals(request, rustCbor.decodeFromByteArray<GalleryListUrlRequest>(requestBytes))
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(EXPECTED_URL, actual)
  }

  @Test
  fun everySiteAndModeUsesTheRustWireDiscriminant() {
    val sites = listOf(GalleryUrlSite.PUBLIC to "public", GalleryUrlSite.EX_HENTAI to "exHentai")
    sites.forEach { (site, expected) ->
      assertEquals(expected, rustCbor.decodeFromByteArray<String>(rustCbor.encodeToByteArray(site)))
    }

    val modes =
      listOf(
        GalleryListMode.NORMAL to "normal",
        GalleryListMode.UPLOADER to "uploader",
        GalleryListMode.TAG to "tag",
        GalleryListMode.WHATS_HOT to "whatsHot",
        GalleryListMode.IMAGE_SEARCH to "imageSearch",
        GalleryListMode.SUBSCRIPTION to "subscription",
        GalleryListMode.TOPLIST to "toplist",
      )
    modes.forEach { (mode, expected) ->
      assertEquals(expected, rustCbor.decodeFromByteArray<String>(rustCbor.encodeToByteArray(mode)))
    }
  }

  @Test
  fun rejectsWrongEnvelopeTypedFailureMissingPayloadAndBlankUrl() {
    val plan = GalleryUrlPlan(EXPECTED_URL)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryListUrl", payload = plan).toGalleryListUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = plan).toGalleryListUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryUrlPlan>(version = 1, module = "galleryListUrl").toGalleryListUrl()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryListUrl", payload = GalleryUrlPlan(""))
        .toGalleryListUrl()
    }

    val failure = RustError(RustErrorKind.ERROR, "missing URL field")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryUrlPlan>(
            version = 1,
            module = "galleryListUrl",
            error = failure,
          )
          .toGalleryListUrl()
      }
    assertEquals(failure, error.failure)
  }

  private fun goldenRequest() =
    GalleryListUrlRequest(
      version = 1,
      site = GalleryUrlSite.EX_HENTAI,
      signedIn = true,
      mode = GalleryListMode.TAG,
      previous = null,
      next = null,
      seek = null,
      range = 0,
      category = -1,
      keyword = "artist:alice",
      imageHash = null,
      languageTag = null,
      advancedSearch = -1,
      minimumRating = -1,
      pageFrom = -1,
      pageTo = -1,
    )

  private companion object {
    const val EXPECTED_URL = "https://exhentai.org/tag/artist:alice"
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "b06776657273696f6e01647369746568657848656e746169687369676e6564496ef5646d6f64" +
        "65637461676870726576696f7573f6646e657874f6647365656bf66572616e67650068636174" +
        "65676f727920676b6579776f72646c6172746973743a616c69636569696d61676548617368f6" +
        "6b6c616e6775616765546167f66e616476616e636564536561726368206d6d696e696d756d52" +
        "6174696e6720687061676546726f6d206670616765546f20"
    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e67616c6c6572794c69737455726c677061796c6f" +
        "6164a16375726c782568747470733a2f2f657868656e7461692e6f72672f7461672f61727469" +
        "73743a616c696365656572726f72f6"
  }
}
