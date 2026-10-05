package com.hippo.ehviewer.image

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import coil3.BitmapImage
import coil3.DrawableImage
import com.hippo.ehviewer.coil.AnimatedWebPDrawable
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Ensures decoded reader images do not retain Java-owned extraction buffers unnecessarily. */
@RunWith(AndroidJUnit4::class)
class ImageMemoryDeviceTest {
  @Test
  fun staticBitmapReleasesSourceImmediatelyAndOnlyOnce() = runBlocking {
    val bitmap = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888)
    val encoded =
      try {
        ByteArrayOutputStream().use { output ->
          bitmap.eraseColor(Color.BLUE)
          assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output))
          output.toByteArray()
        }
      } finally {
        bitmap.recycle()
      }
    var releases = 0
    val source = byteBufferSource(directBuffer(encoded)) { releases++ }
    val image = Image.decode(source)
    try {
      val decoded = image.innerImage as BitmapImage
      assertEquals(1, releases)
      assertThrows(IllegalStateException::class.java) { source.source }
      assertEquals(decoded.bitmap.allocationByteCount.toLong(), image.allocationSize)
    } finally {
      assertTrue(image.unpin())
    }
    assertEquals(1, releases)
    assertNull(image.innerImage)
  }

  @Test
  fun ownedWebPReleasesSourceAndAccountsForAllFrameBuffers() = runBlocking {
    val hex =
      InstrumentationRegistry.getInstrumentation().context.assets.open("animated.webp.hex").use {
        it.bufferedReader().readText()
      }
    val digits = hex.filterNot(Char::isWhitespace)
    val encoded =
      ByteArray(digits.length / 2) { index ->
        digits.substring(index * 2, index * 2 + 2).toInt(16).toByte()
      }
    var releases = 0
    val source = byteBufferSource(directBuffer(encoded)) { releases++ }
    val image = Image.decode(source)
    val output = Bitmap.createBitmap(400, 400, Bitmap.Config.ARGB_8888)
    try {
      val drawable = (image.innerImage as DrawableImage).drawable as AnimatedWebPDrawable
      assertEquals(1, releases)
      assertThrows(IllegalStateException::class.java) { source.source }
      assertEquals(16L * 400 * 400 + encoded.size, image.allocationSize)
      output.eraseColor(Color.MAGENTA)
      drawable.setBounds(0, 0, output.width, output.height)
      drawable.draw(Canvas(output))
      val pixels = IntArray(output.width * output.height)
      output.getPixels(pixels, 0, output.width, 0, 0, output.width, output.height)
      assertTrue(pixels.any { it != Color.MAGENTA })
    } finally {
      image.unpin()
      output.recycle()
    }
    assertEquals(1, releases)
    assertNull(image.innerImage)
  }

  private fun directBuffer(bytes: ByteArray): ByteBuffer =
    ByteBuffer.allocateDirect(bytes.size).apply {
      put(bytes)
      flip()
    }
}
