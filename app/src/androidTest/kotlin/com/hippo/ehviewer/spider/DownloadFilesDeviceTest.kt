package com.hippo.ehviewer.spider

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.files.expectedImageHash
import com.hippo.ehviewer.bridge.files.planPostArchivePages
import com.hippo.ehviewer.bridge.files.rustPageFilename
import java.text.DecimalFormatSymbols
import java.util.Locale
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DownloadFilesDeviceTest {
  @Test
  fun localeAndRawUtf16NamingMatchesPlatformFormatter() {
    for (tag in listOf("en-US", "ar-EG", "fa-IR", "bn-BD", "th-TH-u-nu-thai")) {
      val locale = Locale.forLanguageTag(tag)
      val zero = DecimalFormatSymbols.getInstance(locale).zeroDigit
      for (index in listOf(0, -1, -2, Int.MIN_VALUE, Int.MAX_VALUE)) {
        for (ext in listOf("", "jpg", "\ud800.tmp")) {
          assertEquals(
            String.format(locale, "%08d.%s", index + 1, ext),
            rustPageFilename(index, ext, zero),
          )
        }
      }
    }
  }

  @Test
  fun usesTheActualAndroidRegexFactsBeforeCleanup() {
    val names =
      listOf(
        "00000001.jpg",
        "٠٠٠٠٠٠٠١.jpg",
        "00000001.abc",
        "00000001.jpeg.tmp",
        "00000000.JPG",
        "note.txt",
      )
    val regex = Regex("^\\d{8}\\.\\w{3,4}")
    val facts = BooleanArray(names.size) { names[it].matches(regex) }
    assertArrayEquals(
      names.indices.filter { names[it].matches(regex) }.toIntArray(),
      planPostArchivePages(facts),
    )
    val hash = "a".repeat(40)
    assertEquals(hash, expectedImageHash("\ud800/h/${hash}f"))
  }
}
