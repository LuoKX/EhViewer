package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryToken
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryTokenBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixtureWithoutInterpretingTheToken() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryToken>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals("0439fa3666", envelope.toGalleryToken())
  }

  @Test
  fun preservesEmptyOpaqueAndUnicodeTokens() {
    for (token in listOf("", "opaque token", "令牌")) {
      assertEquals(token, successEnvelope(token).toGalleryToken())
    }
  }

  @Test
  fun rejectsInvalidContractsAndPreservesTypedFailures() {
    assertFailsWith<IllegalArgumentException> {
      successEnvelope("token").copy(version = 2).toGalleryToken()
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope("token").copy(module = "gallery").toGalleryToken()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryToken>(version = 1, module = "galleryToken")
    }

    val failure = RustError(RustErrorKind.ERROR, "Gallery token response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryToken>(version = 1, module = "galleryToken", error = failure)
          .toGalleryToken()
      }
    assertEquals(failure, exception.failure)
  }

  private fun successEnvelope(token: String) =
    ApiEnvelope(version = 1, module = "galleryToken", payload = GalleryToken(token))

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656c67616c6c657279546f6b656e677061796c6f6164" +
        "a165746f6b656e6a30343339666133363636656572726f72f6"
  }
}
