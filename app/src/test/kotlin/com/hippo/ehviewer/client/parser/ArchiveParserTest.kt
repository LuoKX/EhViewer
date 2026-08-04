package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveDownloadTarget
import com.hippo.ehviewer.bridge.ArchiveFunds
import com.hippo.ehviewer.bridge.ArchiveListPage
import com.hippo.ehviewer.bridge.ArchiveOption
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

class ArchiveParserTest {
  @Test
  fun defaultOffListPathCallsOnlyTheLegacyParser() {
    var legacyCalled = false
    var rustCalled = false
    val expected = ArchiveParser.Result(emptyList(), Funds(1, 2))

    val result =
      ArchiveParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        maybeFunds = expected.funds,
        useRust = false,
        legacyParser = { _, _ -> expected.also { legacyCalled = true } },
        rustParser = { _, _ ->
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertEquals(expected, result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledListPathRequestsFundsOnlyWhenAndroidHasNone() {
    var legacyCalled = false
    val body = ByteBuffer.allocateDirect(8)
    val parsed =
      ArchiveParser.parseWithAdapters(
        body = body,
        maybeFunds = null,
        useRust = true,
        legacyParser = { _, _ ->
          legacyCalled = true
          error("Legacy parser must not run")
        },
        rustParser = { input, includeFunds ->
          assertEquals(body, input)
          assertTrue(includeFunds)
          listEnvelope(ArchiveFunds(3, 4))
        },
      )

    assertFalse(legacyCalled)
    assertEquals(Funds(3, 4), parsed.funds)

    val external = Funds(5, 6)
    val withExternal =
      ArchiveParser.parseWithAdapters(
        body = body,
        maybeFunds = external,
        useRust = true,
        legacyParser = { _, _ -> error("Legacy parser must not run") },
        rustParser = { _, includeFunds ->
          assertFalse(includeFunds)
          listEnvelope(null)
        },
      )
    assertEquals(external, withExternal.funds)
  }

  @Test
  fun rustBusinessFailureNeverFallsBackToTheLegacyListParser() {
    var legacyCalled = false
    val failure = RustError(RustErrorKind.NO_HATH_CLIENT)

    val error =
      assertFailsWith<RustFailureException> {
        ArchiveParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          maybeFunds = null,
          useRust = true,
          legacyParser = { _, _ ->
            legacyCalled = true
            ArchiveParser.Result(emptyList(), Funds(0, 0))
          },
          rustParser = { _, _ ->
            ApiEnvelope(version = 1, module = "galleryArchive", error = failure)
          },
        )
      }

    assertEquals(failure, error.failure)
    assertFalse(legacyCalled)
  }

  @Test
  fun selectedParserTransportFailuresKeepTheExistingParseExceptionBoundary() {
    val listError =
      assertFailsWith<ParseException> {
        ArchiveParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          maybeFunds = null,
          useRust = true,
          legacyParser = { _, _ -> error("Legacy parser must not run") },
          rustParser = { _, _ -> error("transport") },
        )
      }
    assertTrue(listError.message.orEmpty().contains("archive list"))

    val urlError =
      assertFailsWith<ParseException> {
        ArchiveParser.parseUrlWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = false,
          legacyParser = { error("transport") },
          rustParser = { error("Rust parser must not run") },
        )
      }
    assertTrue(urlError.message.orEmpty().contains("archive url"))
  }

  @Test
  fun targetSelectionNeverFallsBackFromRustToTheUnsafeLegacyParser() {
    var legacyCalled = false
    val body = ByteBuffer.allocateDirect(8)
    val rustUrl =
      ArchiveParser.parseUrlWithAdapters(
        body = body,
        useRust = true,
        legacyParser = {
          legacyCalled = true
          "javascript:legacy"
        },
        rustParser = {
          ApiEnvelope(
            version = 1,
            module = "galleryArchive",
            payload = ArchiveDownloadTarget("https://archive.example/file?start=1"),
          )
        },
      )
    assertEquals("https://archive.example/file?start=1", rustUrl)
    assertFalse(legacyCalled)

    val failure = RustError(RustErrorKind.ERROR, "unsafe archive target")
    assertFailsWith<RustFailureException> {
      ArchiveParser.parseUrlWithAdapters(
        body = body,
        useRust = true,
        legacyParser = {
          legacyCalled = true
          "javascript:legacy"
        },
        rustParser = { ApiEnvelope(version = 1, module = "galleryArchive", error = failure) },
      )
    }
    assertFalse(legacyCalled)
  }

  private fun listEnvelope(funds: ArchiveFunds?) =
    ApiEnvelope(
      version = 1,
      module = "galleryArchive",
      payload =
        ArchiveListPage(
          options =
            listOf(
              ArchiveOption(
                resolution = "org",
                name = "",
                size = "1 GiB",
                cost = "100 GP",
                isHath = false,
              )
            ),
          funds = funds,
        ),
    )
}
