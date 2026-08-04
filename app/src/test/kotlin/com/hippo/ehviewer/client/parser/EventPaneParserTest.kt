package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.NewsPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException

class EventPaneParserTest {
  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() {
    var legacyCalled = false
    var rustCalled = false
    val body = ByteBuffer.allocateDirect(8)

    val result =
      EventPaneParser.parseWithAdapters(
        body = body,
        useRust = false,
        legacyParser = {
          assertSame(body, it)
          legacyCalled = true
          "legacy"
        },
        rustParser = {
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertEquals("legacy", result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledPathInvokesOnlyRustAndPreservesOptionalHtml() {
    var legacyCalled = false
    val body = ByteBuffer.allocateDirect(8)

    for (eventHtml in listOf(null, "", "<strong>Event</strong>")) {
      val result =
        EventPaneParser.parseWithAdapters(
          body = body,
          useRust = true,
          legacyParser = {
            legacyCalled = true
            error("Legacy parser must not run")
          },
          rustParser = {
            assertSame(body, it)
            ApiEnvelope(version = 1, module = "news", payload = NewsPage(eventHtml))
          },
        )
      assertEquals(eventHtml, result)
    }
    assertFalse(legacyCalled)
  }

  @Test
  fun rustBusinessFailureAndCancellationNeverReplayLegacy() {
    var legacyCalled = false
    val failure = RustError(RustErrorKind.ERROR, "news response HTML is invalid")

    val error =
      assertFailsWith<RustFailureException> {
        EventPaneParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = {
            legacyCalled = true
            "legacy"
          },
          rustParser = { ApiEnvelope(version = 1, module = "news", error = failure) },
        )
      }
    assertEquals(failure, error.failure)
    assertFalse(legacyCalled)

    assertFailsWith<CancellationException> {
      EventPaneParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        useRust = true,
        legacyParser = {
          legacyCalled = true
          "legacy"
        },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertFalse(legacyCalled)
  }

  @Test
  fun selectedParserTransportFailureKeepsTheParseExceptionBoundary() {
    val cause = IllegalStateException("transport")
    val error =
      assertFailsWith<ParseException> {
        EventPaneParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = { error("Legacy parser must not run") },
          rustParser = { throw cause },
        )
      }

    assertTrue(error.message.orEmpty().contains("news"))
    assertSame(cause, error.cause)
  }
}
