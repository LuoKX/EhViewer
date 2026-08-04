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

class GalleryNavigationBridgeTest {
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
    val expected = GalleryNavigationPlan.page(42, "0123456789", 2)
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryNavigationPlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )

    assertEquals(
      expected,
      envelope.toGalleryNavigationPlan(GalleryNavigationTarget.PAGE),
    )
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithTheCompleteRequest() {
    val request = goldenRequest()
    val expected = GalleryNavigationPlan.page(42, "0123456789", 2)
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(version = 1, module = "galleryNavigation", payload = expected)
      )
    var invocationCount = 0

    val actual =
      parseGalleryNavigation(request) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        assertEquals(
          request,
          rustCbor.decodeFromByteArray<GalleryNavigationRequest>(requestBytes),
        )
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(expected, actual)
  }

  @Test
  fun acceptsTheExactNoMatchPlanForEitherRequestedGrammar() {
    val none = GalleryNavigationPlan.none()
    for (target in GalleryNavigationTarget.entries) {
      assertEquals(
        none,
        ApiEnvelope(version = 1, module = "galleryNavigation", payload = none)
          .toGalleryNavigationPlan(target),
      )
    }
  }

  @Test
  fun rejectsInvalidPlanFieldsBeforeNavigation() {
    assertFailsWith<IllegalArgumentException> {
      GalleryNavigationPlan(GalleryNavigationKind.NONE, gid = 42)
    }
    assertFailsWith<IllegalArgumentException> {
      GalleryNavigationPlan.detail(-1, "0123456789")
    }
    assertFailsWith<IllegalArgumentException> {
      GalleryNavigationPlan.detail(42, "ABCDEF0123")
    }
    assertFailsWith<IllegalArgumentException> {
      GalleryNavigationPlan.page(42, "0123456789", -2)
    }
  }

  @Test
  fun rejectsWrongEnvelopeTargetTypedFailureAndMissingPayload() {
    val detail = GalleryNavigationPlan.detail(42, "0123456789")
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryNavigation", payload = detail)
        .toGalleryNavigationPlan(GalleryNavigationTarget.DETAIL)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = detail)
        .toGalleryNavigationPlan(GalleryNavigationTarget.DETAIL)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryNavigationPlan>(version = 1, module = "galleryNavigation")
        .toGalleryNavigationPlan(GalleryNavigationTarget.DETAIL)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          version = 1,
          module = "galleryNavigation",
          payload = GalleryNavigationPlan.page(42, "0123456789", 0),
        )
        .toGalleryNavigationPlan(GalleryNavigationTarget.DETAIL)
    }

    val failure = RustError(RustErrorKind.ERROR, "gallery-navigation input is invalid")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryNavigationPlan>(
            version = 1,
            module = "galleryNavigation",
            error = failure,
          )
          .toGalleryNavigationPlan(GalleryNavigationTarget.DETAIL)
      }
    assertEquals(failure, error.failure)
  }

  private fun goldenRequest() =
    GalleryNavigationRequest(
      version = 1,
      target = GalleryNavigationTarget.PAGE,
      input = "https://e-hentai.org/s/0123456789/42-3",
      strict = true,
    )

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a46776657273696f6e0166746172676574647061676565696e707574782668747470733a2f2f" +
        "652d68656e7461692e6f72672f732f303132333435363738392f34322d3366737472696374f5"
    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657167616c6c6572794e617669676174696f6e677061" +
        "796c6f6164a4646b696e64647061676563676964182a65746f6b656e6a303132333435363738" +
        "396970616765496e64657802656572726f72f6"
  }
}
