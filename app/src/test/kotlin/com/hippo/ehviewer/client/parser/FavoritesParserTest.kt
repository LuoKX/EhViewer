package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CancellationException

class FavoritesParserTest {
  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() {
    var legacyCalls = 0
    var rustCalls = 0
    val result =
      FavoritesParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
          legacyCalls += 1
          result("legacy")
        },
        rustParser = {
          rustCalls += 1
          error("V1 parser must remain disabled")
        },
      )

    assertEquals("legacy", result.catArray.single())
    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)
  }

  @Test
  fun enabledPathInvokesOnlyV1AndNeverReplaysAfterFailureOrCancellation() {
    var legacyCalls = 0
    val success =
      FavoritesParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          result("legacy")
        },
        rustParser = { result("rust") },
      )
    assertEquals("rust", success.catArray.single())
    assertEquals(0, legacyCalls)

    val failure = RustError(RustErrorKind.ERROR, "Gallery favorites response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        FavoritesParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalls += 1
            result("legacy")
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertEquals(0, legacyCalls)

    assertFailsWith<CancellationException> {
      FavoritesParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          result("legacy")
        },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertEquals(0, legacyCalls)
  }

  private fun result(category: String) =
    FavParserResult(
      catArray = listOf(category),
      countArray = listOf(1),
      galleryListResult = GalleryListResult(null, null, arrayListOf()),
    )
}
