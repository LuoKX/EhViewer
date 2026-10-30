package com.hippo.ehviewer.client

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.gallery.CommentLink
import com.hippo.ehviewer.bridge.gallery.commentImageDimensions
import com.hippo.ehviewer.bridge.gallery.commentLinks
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CommentDisplayDeviceTest {
  @Test
  fun fixedGrammarMatchesActualAndroidIcuAndUtf16Offsets() {
    val regex =
      Regex(
        "(http|https)://[a-z0-9A-Z%-]+(\\.[a-z0-9A-Z%-]+)+(:\\d{1,5})?(/[a-zA-Z0-9-_~:#@!&',;=%/*.?+$\\[\\]()]+)?/?"
      )
    for (port in listOf("", ":123456", ":١٢٣", ":𝟘𝟙𝟚𝟛𝟜𝟝", ":\ud800")) {
      val text = "😀 \ud800 https://a.b$port/a?b=1#c (http://c.d/)"
      val expected =
        regex.findAll(text).map { CommentLink(it.range.first, it.range.last + 1) }.toList()
      assertEquals(expected, commentLinks(text))
    }
  }

  @Test
  fun imageNumbersUsePlatformDigitSemanticsAndPreserveOverflow() {
    val regex = Regex("(?:[^-]+-){2}(\\d+)-(\\d+)-[^_]+_([^.]+)")
    for (source in
      listOf(
        "a-b-100-200-c_401.jpg",
        "a-b-١٢-٢٣-c_٤٥.jpg",
        "a-b-2147483648-1-c_200",
        "a-b-𝟘-1-c_200",
        "a-b-1-2-c_-2147483648",
        "none",
      )) {
      var failed = false
      val expected =
        try {
          regex.find(source)?.let {
            val width = it.groupValues[1].toInt()
            val height = it.groupValues[2].toInt()
            val target = it.groupValues[3].toIntOrNull() ?: 200
            intArrayOf(width, height, target, target / 2 * 3)
          }
        } catch (_: NumberFormatException) {
          failed = true
          null
        }
      try {
        val actual = commentImageDimensions(source)
        assertEquals(false, failed)
        assertArrayEquals(expected, actual)
      } catch (_: NumberFormatException) {
        assertEquals(true, failed)
      }
    }
  }
}
