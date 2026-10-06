package com.hippo.ehviewer.coil

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.nio.ByteBuffer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Device contract tests for the Rust-owned animated WebP decoder and Bitmap adapter. */
@RunWith(AndroidJUnit4::class)
class AnimatedWebPDrawableDeviceTest {
  @Test
  fun remainingBytesDecodeDrawAndDisposeWithoutChangingTheSource() {
    val encoded = loadAnimatedWebP()
    val source = ByteBuffer.allocate(encoded.size + 2)
    source.put(0x7f.toByte())
    source.put(encoded)
    source.put(0x7e.toByte())
    source.position(1)
    source.limit(encoded.size + 1)

    val drawable = AnimatedWebPDrawable(source)
    val output = Bitmap.createBitmap(EXPECTED_SIZE, EXPECTED_SIZE, Bitmap.Config.ARGB_8888)
    try {
      assertEquals(EXPECTED_SIZE, drawable.intrinsicWidth)
      assertEquals(EXPECTED_SIZE, drawable.intrinsicHeight)
      assertEquals(1, source.position())
      assertEquals(encoded.size + 1, source.limit())

      output.eraseColor(Color.MAGENTA)
      drawable.setBounds(0, 0, output.width, output.height)
      drawable.draw(Canvas(output))
      val pixels = IntArray(output.width * output.height)
      output.getPixels(pixels, 0, output.width, 0, 0, output.width, output.height)
      assertTrue(
        "The first decoded frame must change the destination",
        pixels.any { it != Color.MAGENTA },
      )

      drawable.start()
      assertTrue(drawable.isRunning)
      drawable.stop()
      assertFalse(drawable.isRunning)
      drawable.start()
      assertTrue(drawable.isRunning)
    } finally {
      drawable.dispose()
      drawable.dispose()
      output.recycle()
    }
    assertFalse(drawable.isRunning)
  }

  @Test
  fun malformedInputFailsBeforeAHandleIsExposed() {
    val malformed =
      ByteBuffer.allocateDirect(4).apply {
        putInt(0x1020_3040)
        flip()
      }
    val error =
      assertThrows(RuntimeException::class.java) {
        AnimatedWebPDrawable(malformed)
      }
    assertTrue(error.message.orEmpty().contains("WebP"))
  }

  @Test
  fun realJniMatchesPremultipliedGoldenFramesAndReset() {
    val assets = InstrumentationRegistry.getInstrumentation().context.assets
    val encoded = assets.open("webp-p3/lossless-blend.webp").use { it.readBytes() }
    val expected = assets.open("webp-p3/lossless-blend.rgba").use { it.readBytes() }
    val bridge = Class.forName("com.hippo.ehviewer.coil.AnimatedWebPDrawableKt")
    fun method(name: String, vararg types: Class<*>) =
      bridge.getDeclaredMethod(name, *types).apply { isAccessible = true }
    val create = method("nativeCreateDecoder", ByteBuffer::class.java)
    val info = method("nativeGetImageInfo", java.lang.Long.TYPE)
    val decode =
      method(
        "nativeDecodeNextFrame",
        java.lang.Long.TYPE,
        java.lang.Boolean.TYPE,
        Bitmap::class.java,
      )
    val destroy = method("nativeDestroyDecoder", java.lang.Long.TYPE)
    val source =
      ByteBuffer.allocateDirect(encoded.size).apply {
        put(encoded)
        flip()
      }
    val handle = (create.invoke(null, source) as Number).toLong()
    val bitmap = Bitmap.createBitmap(6, 4, Bitmap.Config.ARGB_8888)
    try {
      val packed = (info.invoke(null, handle) as Number).toLong()
      assertEquals(6L, packed ushr 40)
      assertEquals(4L, packed ushr 16 and 0xFFFFFF)
      assertEquals(3L, packed and 0xFFFF)
      val timestamps = intArrayOf(13, 50, 50, 79, 86, 105)
      for (index in timestamps.indices) {
        assertEquals(
          timestamps[index],
          (decode.invoke(null, handle, false, bitmap) as Number).toInt(),
        )
        val actual = ByteBuffer.allocate(bitmap.byteCount)
        bitmap.copyPixelsToBuffer(actual)
        assertArrayEquals(expected.copyOfRange(index * 96, (index + 1) * 96), actual.array())
      }
      assertEquals(13, (decode.invoke(null, handle, true, bitmap) as Number).toInt())
    } finally {
      destroy.invoke(null, handle)
      destroy.invoke(null, handle)
      bitmap.recycle()
    }
  }

  private fun loadAnimatedWebP(): ByteArray {
    val hex =
      InstrumentationRegistry.getInstrumentation().context.assets.open(ANIMATED_WEBP_ASSET).use {
        it.bufferedReader().readText()
      }
    val digits = hex.filterNot(Char::isWhitespace)
    require(digits.length % 2 == 0) { "Animated WebP fixture must contain byte pairs" }
    return ByteArray(digits.length / 2) { index ->
      digits.substring(index * 2, index * 2 + 2).toInt(16).toByte()
    }
  }

  private companion object {
    const val ANIMATED_WEBP_ASSET = "animated.webp.hex"
    const val EXPECTED_SIZE = 400
  }
}
