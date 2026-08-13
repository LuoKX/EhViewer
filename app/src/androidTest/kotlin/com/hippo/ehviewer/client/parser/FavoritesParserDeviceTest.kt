package com.hippo.ehviewer.client.parser

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI source for the sole production favorite-note response parser. */
@RunWith(AndroidJUnit4::class)
class FavoritesParserDeviceTest {
  @Test
  fun parsesAndRejectsThroughTheDefaultV1Boundary() {
    assertEquals(
      " 日本語 & × ",
      FavoritesParser.parseNote(directBody("<textarea id=n> 日本語 &amp; &times; </textarea>")),
    )
    assertThrows(ParseException::class.java) {
      FavoritesParser.parseNote(directBody("not a favorite-note response"))
    }
  }

  private fun directBody(value: String) =
    ByteBuffer.allocateDirect(512).apply {
      put(value.encodeToByteArray())
      flip()
    }
}
