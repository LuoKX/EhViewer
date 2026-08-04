package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.client.exception.ParseException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class GalleryPageParserTest {
  @Test
  fun legacyParserPreservesCompleteAndMinimalReaderPageBehavior() {
    val complete =
      GalleryPageParser.parse(
        """
        <img src="https://img.example/page.jpg?x=1&amp;y=2" style="width:1px">
        <a onclick="return nl('skip')">Retry</a>
        <a href="https://img.example/fullimg/42/page.jpg?x=1&amp;y=2">Original</a>
        <script>var showkey="show123";</script>
        """
          .trimIndent()
      )
    assertEquals("https://img.example/page.jpg?x=1&y=2", complete.imageUrl)
    assertEquals("skip", complete.skipHathKey)
    assertEquals("https://img.example/fullimg/42/page.jpg?x=1&y=2", complete.originImageUrl)
    assertEquals("show123", complete.showKey)

    val minimal = GalleryPageParser.parse("<img src=\"minimal.jpg\" style=\"width:1px\">")
    assertEquals("minimal.jpg", minimal.imageUrl)
    assertEquals(null, minimal.skipHathKey)
    assertEquals(null, minimal.originImageUrl)
    assertEquals(null, minimal.showKey)
  }

  @Test
  fun legacyParserRejectsMissingImageMarkup() {
    assertFailsWith<ParseException> { GalleryPageParser.parse("<html></html>") }
  }
}
