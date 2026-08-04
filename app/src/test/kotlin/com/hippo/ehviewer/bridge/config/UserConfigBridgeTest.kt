package com.hippo.ehviewer.bridge.config

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.FavoriteCategories
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class UserConfigBridgeTest {
  @Test
  fun decodesAndMapsTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<FavoriteCategories>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertContentEquals(arrayOf("One & More", "Two"), envelope.toFavoriteCategoryArray())
  }

  @Test
  fun preservesEmptyPartialDuplicateAndWhitespacePayloads() {
    for (names in listOf(emptyList(), listOf("  first  ", "first"))) {
      val result =
        ApiEnvelope(
            version = 1,
            module = "userConfig",
            payload = FavoriteCategories(names),
          )
          .toFavoriteCategoryArray()
      assertContentEquals(names.toTypedArray(), result)
    }
  }

  @Test
  fun rejectsUnsupportedContractsAndPreservesTypedFailures() {
    val payload = FavoriteCategories(listOf("one"))
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "userConfig", payload = payload).toFavoriteCategoryArray()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "config", payload = payload).toFavoriteCategoryArray()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<FavoriteCategories>(version = 1, module = "userConfig").toFavoriteCategoryArray()
    }

    val failure = RustError(RustErrorKind.ERROR, "invalid user config")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<FavoriteCategories>(version = 1, module = "userConfig", error = failure)
          .toFavoriteCategoryArray()
      }
    kotlin.test.assertEquals(failure, error.failure)
  }

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656a75736572436f6e666967677061796c6f6164a165" +
        "6e616d6573826a4f6e652026204d6f72656354776f656572726f72f6"
  }
}
