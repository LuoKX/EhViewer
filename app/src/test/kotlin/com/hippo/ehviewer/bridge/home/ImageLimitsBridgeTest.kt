package com.hippo.ehviewer.bridge.home

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ImageLimits
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class ImageLimitsBridgeTest {
  @Test
  fun decodesAndMapsTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<ImageLimits>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val limits = envelope.toLegacyLimits()

    assertEquals(1, envelope.version)
    assertEquals("imageLimits", envelope.module)
    assertEquals(1_234, limits.current)
    assertEquals(5_000, limits.maximum)
    assertEquals(12_345, limits.resetCost)
  }

  @Test
  fun preservesRestrictedAndUnrestrictedMaximumSentinels() {
    for (maximum in listOf(-1, 0)) {
      val limits =
        ApiEnvelope(
            version = 1,
            module = "imageLimits",
            payload = ImageLimits(current = 0, maximum = maximum, resetCost = 0),
          )
          .toLegacyLimits()
      assertEquals(maximum, limits.maximum)
    }
  }

  @Test
  fun rejectsUnsupportedContractsAndPreservesTypedFailures() {
    val payload = ImageLimits(current = 1, maximum = 2, resetCost = 3)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "imageLimits", payload = payload).toLegacyLimits()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "home", payload = payload).toLegacyLimits()
    }

    val failure = RustError(RustErrorKind.ERROR, "image-limits panel is missing")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<ImageLimits>(version = 1, module = "imageLimits", error = failure)
          .toLegacyLimits()
      }
    assertEquals(failure, error.failure)
  }

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656b696d6167654c696d697473677061796c6f6164a367" +
        "63757272656e741904d2676d6178696d756d191388697265736574436f7374193039656572726f72" +
        "f6"
  }
}
