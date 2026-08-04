package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryRating
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryRateBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryRating>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val result = envelope.toRateGalleryResult()
    assertEquals(4.75F, result.rating)
    assertEquals(128, result.ratingCount)
  }

  @Test
  fun preservesFloatBitsAndSignedCountsWithoutAddingAndroidPolicy() {
    val negativeZero =
      successEnvelope(rating = -0.0F, ratingCount = Int.MIN_VALUE).toRateGalleryResult()
    assertEquals((-0.0F).toBits(), negativeZero.rating.toBits())
    assertEquals(Int.MIN_VALUE, negativeZero.ratingCount)

    val maximum =
      successEnvelope(rating = Float.MAX_VALUE, ratingCount = Int.MAX_VALUE).toRateGalleryResult()
    assertEquals(Float.MAX_VALUE, maximum.rating)
    assertEquals(Int.MAX_VALUE, maximum.ratingCount)
  }

  @Test
  fun rejectsVersionAndModuleDrift() {
    val envelope = successEnvelope(rating = 3.5F, ratingCount = 7)

    assertFailsWith<IllegalArgumentException> {
      envelope.copy(version = 2).toRateGalleryResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(module = "galleryTagVote").toRateGalleryResult()
    }
  }

  @Test
  fun preservesTypedRustFailuresForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "Gallery rating response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryRating>(version = 1, module = "galleryRate", error = failure)
          .toRateGalleryResult()
      }

    assertEquals(failure, exception.failure)
  }

  private fun successEnvelope(rating: Float, ratingCount: Int) =
    ApiEnvelope(
      version = 1,
      module = "galleryRate",
      payload = GalleryRating(rating = rating, ratingCount = ratingCount),
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656b67616c6c6572795261746567706179" +
        "6c6f6164a266726174696e67f944c06b726174696e67436f756e74188065657272" +
        "6f72f6"
  }
}
