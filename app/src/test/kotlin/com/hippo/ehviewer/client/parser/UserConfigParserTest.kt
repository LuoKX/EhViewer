package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.FavoriteCategories
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class UserConfigParserTest {
  @Test
  fun defaultOffPathCallsOnlyTheLegacyParser() {
    var legacyCalled = false
    var rustCalled = false
    val expected = arrayOf("legacy")

    val result =
      UserConfigParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        useRust = false,
        legacyParser = { expected.also { legacyCalled = true } },
        rustParser = {
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertContentEquals(expected, result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledPathCallsOnlyRustAndPreservesPartialPayload() {
    var legacyCalled = false
    val body = ByteBuffer.allocateDirect(8)

    val result =
      UserConfigParser.parseWithAdapters(
        body = body,
        useRust = true,
        legacyParser = {
          legacyCalled = true
          error("Legacy parser must not run")
        },
        rustParser = { input ->
          kotlin.test.assertEquals(body, input)
          ApiEnvelope(
            version = 1,
            module = "userConfig",
            payload = FavoriteCategories(listOf("first", "first")),
          )
        },
      )

    assertContentEquals(arrayOf("first", "first"), result)
    assertFalse(legacyCalled)
  }

  @Test
  fun rustFailureNeverFallsBackToLegacy() {
    var legacyCalled = false
    val failure = RustError(RustErrorKind.ERROR, "invalid user config")

    val error =
      assertFailsWith<RustFailureException> {
        UserConfigParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = {
            legacyCalled = true
            emptyArray()
          },
          rustParser = {
            ApiEnvelope(version = 1, module = "userConfig", error = failure)
          },
        )
      }

    kotlin.test.assertEquals(failure, error.failure)
    assertFalse(legacyCalled)
  }
}
