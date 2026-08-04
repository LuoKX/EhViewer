package com.hippo.ehviewer.image

import android.graphics.Bitmap
import android.graphics.Color
import android.hardware.HardwareBuffer
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Device contract tests for the Rust-backed Bitmap-to-HardwareBuffer adapter. */
@RunWith(AndroidJUnit4::class)
@SdkSuppress(minSdkVersion = Build.VERSION_CODES.Q)
class HardwareBufferCopyTest {
  @Test
  fun argb8888CopyPreservesPixelsAndBufferLifetime() {
    verifyFormat(FormatCase(Bitmap.Config.ARGB_8888, HardwareBuffer.RGBA_8888, 0))
  }

  @Test
  fun rgb565CopyPreservesPixelsAndBufferLifetime() {
    verifyFormat(FormatCase(Bitmap.Config.RGB_565, HardwareBuffer.RGB_565, 8))
  }

  @Test
  fun rgbaF16CopyPreservesPixelsAndBufferLifetime() {
    verifyFormat(FormatCase(Bitmap.Config.RGBA_F16, HardwareBuffer.RGBA_FP16, 2))
  }

  @Test
  fun invalidCoordinatesAreContainedAndObjectsRemainReusable() {
    val formatCase = FormatCase(Bitmap.Config.ARGB_8888, HardwareBuffer.RGBA_8888, 0)
    verifyFormat(formatCase) { source, buffer ->
      val error =
        assertThrows(RuntimeException::class.java) {
          copyBitmapToAHB(source, buffer, -1, CROP_Y)
        }
      assertTrue(error.message.orEmpty().contains("negative source x-coordinate"))
    }
  }

  private fun verifyFormat(
    formatCase: FormatCase,
    beforeValidCopy: (Bitmap, HardwareBuffer) -> Unit = { _, _ -> },
  ) {
    val buffer = createBuffer(formatCase.hardwareBufferFormat)
    var bufferOpen = true
    var source: Bitmap? = null
    var wrapped: Bitmap? = null
    var softwareCopy: Bitmap? = null
    try {
      val sourceBitmap = createSource(formatCase.bitmapConfig)
      source = sourceBitmap
      beforeValidCopy(sourceBitmap, buffer)

      // A second successful lock cycle proves both native guards released the first one.
      copyBitmapToAHB(sourceBitmap, buffer, 0, 0)
      copyBitmapToAHB(sourceBitmap, buffer, CROP_X, CROP_Y)

      val wrappedBitmap =
        requireNotNull(Bitmap.wrapHardwareBuffer(buffer, sourceBitmap.colorSpace)) {
          "Android could not wrap HardwareBuffer format ${formatCase.hardwareBufferFormat}"
        }
      wrapped = wrappedBitmap

      // Bitmap owns its own native reference and must remain usable after this close.
      buffer.close()
      bufferOpen = false
      val copiedBitmap =
        requireNotNull(wrappedBitmap.copy(Bitmap.Config.ARGB_8888, false)) {
          "Android could not read back the wrapped hardware bitmap"
        }
      softwareCopy = copiedBitmap
      assertCropEquals(sourceBitmap, copiedBitmap, formatCase.channelTolerance)
    } finally {
      softwareCopy?.recycle()
      wrapped?.recycle()
      if (bufferOpen) buffer.close()
      source?.recycle()
    }
  }

  private fun createSource(config: Bitmap.Config): Bitmap =
    Bitmap.createBitmap(SOURCE_WIDTH, SOURCE_HEIGHT, config).apply {
      setPixels(sourceColors, 0, SOURCE_WIDTH, 0, 0, SOURCE_WIDTH, SOURCE_HEIGHT)
    }

  private fun createBuffer(format: Int): HardwareBuffer {
    assertTrue(
      "The device must support format $format for the image-copy contract",
      HardwareBuffer.isSupported(DESTINATION_WIDTH, DESTINATION_HEIGHT, format, 1, BUFFER_USAGE),
    )
    return HardwareBuffer.create(
      DESTINATION_WIDTH,
      DESTINATION_HEIGHT,
      format,
      1,
      BUFFER_USAGE,
    )
  }

  private fun assertCropEquals(source: Bitmap, actual: Bitmap, tolerance: Int) {
    assertEquals(DESTINATION_WIDTH, actual.width)
    assertEquals(DESTINATION_HEIGHT, actual.height)
    repeat(DESTINATION_HEIGHT) { row ->
      repeat(DESTINATION_WIDTH) { column ->
        val expected = source.getPixel(CROP_X + column, CROP_Y + row)
        val observed = actual.getPixel(column, row)
        assertColorClose(expected, observed, tolerance, "pixel ($column, $row)")
      }
    }
  }

  private fun assertColorClose(expected: Int, actual: Int, tolerance: Int, label: String) {
    assertTrue(
      "$label alpha differs: expected ${Color.alpha(expected)}, got ${Color.alpha(actual)}",
      abs(Color.alpha(expected) - Color.alpha(actual)) <= tolerance,
    )
    assertTrue(
      "$label red differs: expected ${Color.red(expected)}, got ${Color.red(actual)}",
      abs(Color.red(expected) - Color.red(actual)) <= tolerance,
    )
    assertTrue(
      "$label green differs: expected ${Color.green(expected)}, got ${Color.green(actual)}",
      abs(Color.green(expected) - Color.green(actual)) <= tolerance,
    )
    assertTrue(
      "$label blue differs: expected ${Color.blue(expected)}, got ${Color.blue(actual)}",
      abs(Color.blue(expected) - Color.blue(actual)) <= tolerance,
    )
  }

  private data class FormatCase(
    val bitmapConfig: Bitmap.Config,
    val hardwareBufferFormat: Int,
    val channelTolerance: Int,
  )

  private companion object {
    const val SOURCE_WIDTH = 3
    const val SOURCE_HEIGHT = 3
    const val DESTINATION_WIDTH = 2
    const val DESTINATION_HEIGHT = 2
    const val CROP_X = 1
    const val CROP_Y = 1

    const val BUFFER_USAGE =
      HardwareBuffer.USAGE_CPU_WRITE_RARELY or HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE

    val sourceColors =
      intArrayOf(
        Color.rgb(16, 32, 48),
        Color.rgb(64, 80, 96),
        Color.rgb(112, 128, 144),
        Color.rgb(160, 176, 192),
        Color.rgb(208, 224, 240),
        Color.rgb(18, 52, 86),
        Color.rgb(101, 67, 33),
        Color.rgb(171, 205, 239),
        Color.rgb(34, 68, 102),
      )
  }
}
