package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ImageLimits
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class HomeParserTest {
  @Test
  fun defaultOffPathCallsOnlyTheLegacyParser() {
    var legacyCalled = false
    var rustCalled = false
    val expected = Limits(current = 1, maximum = 2, resetCost = 3)

    val result =
      HomeParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        useRust = false,
        legacyParser = { expected.also { legacyCalled = true } },
        rustParser = {
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertEquals(expected, result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledPathCallsOnlyRustAndMapsEveryField() {
    var legacyCalled = false
    val body = ByteBuffer.allocateDirect(8)

    val result =
      HomeParser.parseWithAdapters(
        body = body,
        useRust = true,
        legacyParser = {
          legacyCalled = true
          error("Legacy parser must not run")
        },
        rustParser = { input ->
          assertEquals(body, input)
          ApiEnvelope(
            version = 1,
            module = "imageLimits",
            payload = ImageLimits(current = 4, maximum = -1, resetCost = 0),
          )
        },
      )

    assertEquals(Limits(current = 4, maximum = -1, resetCost = 0), result)
    assertFalse(legacyCalled)
  }

  @Test
  fun rustBusinessFailureNeverFallsBackToLegacy() {
    var legacyCalled = false
    val failure = RustError(RustErrorKind.ERROR, "invalid image limits")

    val error =
      assertFailsWith<RustFailureException> {
        HomeParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = {
            legacyCalled = true
            Limits(0, 0, 0)
          },
          rustParser = {
            ApiEnvelope(version = 1, module = "imageLimits", error = failure)
          },
        )
      }

    assertEquals(failure, error.failure)
    assertFalse(legacyCalled)
  }

  @Test
  fun selectedParserTransportFailureKeepsTheLegacyParseExceptionBoundary() {
    val error =
      assertFailsWith<ParseException> {
        HomeParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = { error("Legacy parser must not run") },
          rustParser = { error("transport") },
        )
      }
    assertTrue(error.message.orEmpty().contains("image limits"))
  }
}
