package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.SerializationException

class RateGalleryParserTest {
  @Test
  fun legacyParserPreservesUnknownFieldsAndSignedValues() {
    val result =
      RateGalleryParser.parse(
        """{"rating_avg":-0.0,"rating_cnt":-7,"error":"ignored","extra":true}"""
      )

    assertEquals((-0.0F).toBits(), result.rating.toBits())
    assertEquals(-7, result.ratingCount)
  }

  @Test
  fun legacyParserAcceptsNumericStrings() {
    val result = RateGalleryParser.parse("""{"rating_avg":"4.5","rating_cnt":"1"}""")

    assertEquals(RateGalleryResult(rating = 4.5F, ratingCount = 1), result)
  }

  @Test
  fun legacyParserRejectsMissingNullAndMalformedFields() {
    listOf(
        """{"rating_cnt":1}""",
        """{"rating_avg":null,"rating_cnt":1}""",
        """{"rating_avg":4.5,"rating_cnt":null}""",
        """{"rating_avg":4.5,"rating_cnt":}""",
      )
      .forEach { body ->
        assertFailsWith<SerializationException>("Expected parser to reject: $body") {
          RateGalleryParser.parse(body)
        }
      }
  }

  @Test
  fun legacyParserUsesTheLastDuplicateField() {
    val result =
      RateGalleryParser.parse(
        """{"rating_avg":1.0,"rating_avg":4.5,"rating_cnt":2,"rating_cnt":9}"""
      )

    assertEquals(4.5F, result.rating)
    assertEquals(9, result.ratingCount)
  }

  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() = runBlocking {
    var legacyCalls = 0
    var rustCalls = 0
    val result =
      RateGalleryParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
          legacyCalls += 1
          RateGalleryResult(3.0F, 10)
        },
        rustParser = {
          rustCalls += 1
          error("V1 parser must remain disabled")
        },
      )

    assertEquals(RateGalleryResult(3.0F, 10), result)
    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)
  }

  @Test
  fun enabledPathInvokesOnlyV1AndNeverReplaysAfterFailureOrCancellation() = runBlocking {
    var legacyCalls = 0
    val success =
      RateGalleryParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          RateGalleryResult(1.0F, 1)
        },
        rustParser = { RateGalleryResult(4.75F, 128) },
      )
    assertEquals(RateGalleryResult(4.75F, 128), success)
    assertEquals(0, legacyCalls)

    val failure = RustError(RustErrorKind.ERROR, "Gallery rating response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        RateGalleryParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalls += 1
            RateGalleryResult(1.0F, 1)
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertEquals(0, legacyCalls)

    assertFailsWith<CancellationException> {
      RateGalleryParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          RateGalleryResult(1.0F, 1)
        },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertEquals(0, legacyCalls)
  }
}
