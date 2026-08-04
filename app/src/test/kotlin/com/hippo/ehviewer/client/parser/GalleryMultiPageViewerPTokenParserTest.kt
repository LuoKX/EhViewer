package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.exception.ParseException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking

class GalleryMultiPageViewerPTokenParserTest {
  @Test
  fun legacyParserPreservesOrderUnknownFieldsDuplicatesEmptyValuesAndEscapes() {
    val body =
      """
      <script>
      var imagelist = [
        {"k":"0123456789","ignored":1},
        {"k":""},
        {"k":"0123456789"},
        {"k":"unicode-\u00d7"}
      ];
      </script>
      """
        .trimIndent()

    assertEquals(
      listOf("0123456789", "", "0123456789", "unicode-×"),
      GalleryMultiPageViewerPTokenParser.parse(body),
    )
    assertEquals(
      emptyList(),
      GalleryMultiPageViewerPTokenParser.parse("var imagelist = [];"),
    )
  }

  @Test
  fun legacyParserRetainsTheMarkerlessFixedOffsetBugForRollback() {
    assertEquals(
      listOf("legacy-offset"),
      GalleryMultiPageViewerPTokenParser.parse("""123456789012345[{"k":"legacy-offset"}];"""),
    )
  }

  @Test
  fun legacyParserWrapsMissingBoundariesAndInvalidJson() {
    for (body in
      listOf(
        "<html>missing marker</html>",
        "var imagelist = [{\"k\":\"missing terminator\"}]",
        "var imagelist = {};",
        "var imagelist = [{}];",
        "var imagelist = [{\"k\":null}];",
      )) {
      assertFailsWith<ParseException> { GalleryMultiPageViewerPTokenParser.parse(body) }
    }
  }

  @Test
  fun defaultOffPathInvokesOnlyLegacy() = runBlocking {
    var legacyCalled = false
    var rustCalled = false
    val result =
      GalleryMultiPageViewerPTokenParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
          legacyCalled = true
          listOf("legacy")
        },
        rustParser = {
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertEquals(listOf("legacy"), result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledPathInvokesOnlyRustAndNeverFallsBackAfterFailure() = runBlocking {
    var legacyCalled = false
    val result =
      GalleryMultiPageViewerPTokenParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalled = true
          listOf("legacy")
        },
        rustParser = { listOf("rust") },
      )
    assertEquals(listOf("rust"), result)
    assertFalse(legacyCalled)

    val failure = RustError(RustErrorKind.ERROR, "MPV image list is invalid")
    val error =
      assertFailsWith<RustFailureException> {
        GalleryMultiPageViewerPTokenParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalled = true
            listOf("legacy")
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, error.failure)
    assertFalse(legacyCalled)
  }
}
