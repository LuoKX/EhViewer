package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.exception.EhException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.SerializationException

class GalleryTokenApiParserTest {
  @Test
  fun legacyParserReturnsOnlyTheFirstTokenAndIgnoresItsGid() {
    val body =
      """
      {
        "tokenlist": [
          {"gid": -1, "token": "", "ignored": true},
          {"gid": 618395, "token": "later-token"}
        ],
        "ignored": "value"
      }
      """
        .trimIndent()

    assertEquals("", GalleryTokenApiParser.parse(body))
  }

  @Test
  fun legacyParserDecodesTheCompleteListBeforeSelectingTheFirstItem() {
    val exception =
      assertFailsWith<EhException> {
        GalleryTokenApiParser.parse(
          """{"tokenlist":[{"gid":1,"token":"first"},{"gid":"bad","token":"later"}]}"""
        )
      }

    assertNull(exception.message)
  }

  @Test
  fun legacyFallbackPreservesServerAndNullableErrorMessages() {
    assertEquals(
      "remote failure",
      assertFailsWith<EhException> {
          GalleryTokenApiParser.parse("""{"error":"remote failure"}""")
        }
        .message,
    )
    for (body in listOf("{}", """{"error":null}""", """{"tokenlist":[]}""")) {
      assertNull(assertFailsWith<EhException> { GalleryTokenApiParser.parse(body) }.message)
    }
  }

  @Test
  fun legacyFallbackLeavesMalformedErrorShapesAsSerializationFailures() {
    for (body in listOf("not JSON", "[]", "null", """{"error":1}""")) {
      assertFailsWith<SerializationException> { GalleryTokenApiParser.parse(body) }
    }
  }

  @Test
  fun defaultOffPathInvokesOnlyLegacy() = runBlocking {
    var legacyCalled = false
    var rustCalled = false
    val result =
      GalleryTokenApiParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
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
  fun enabledPathInvokesOnlyRustAndNeverFallsBackAfterFailure() = runBlocking {
    var legacyCalled = false
    val result =
      GalleryTokenApiParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalled = true
          "legacy"
        },
        rustParser = { "rust" },
      )
    assertEquals("rust", result)
    assertFalse(legacyCalled)

    val failure = RustError(RustErrorKind.ERROR, "Gallery token response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        GalleryTokenApiParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalled = true
            "legacy"
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertFalse(legacyCalled)

    assertFailsWith<CancellationException> {
      GalleryTokenApiParser.parseWithAdapters(
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
}
