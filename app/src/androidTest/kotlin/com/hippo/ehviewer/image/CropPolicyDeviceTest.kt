package com.hippo.ehviewer.image

import android.graphics.Bitmap
import android.graphics.Color
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CropPolicyDeviceTest {
  @Test
  fun cropMatchesOriginalPolicyOnRealBitmaps() {
    for ((width, height) in
      listOf(32 to 32, 100 to 100, 199 to 100, 200 to 100, 100 to 199, 100 to 200)) {
      for (border in listOf(0, 2, 12, 13)) {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try {
          bitmap.eraseColor(Color.WHITE)
          val pixels = IntArray((width - 2 * border) * (height - 2 * border)) { Color.BLACK }
          bitmap.setPixels(
            pixels,
            0,
            width - 2 * border,
            border,
            border,
            width - 2 * border,
            height - 2 * border,
          )
          val ratio = if (height > width) height / width else width / height
          val expected =
            if (ratio < 2)
              detectBorder(bitmap).takeIf {
                it[2] - it[0] > width * 0.75f && it[3] - it[1] > height * 0.75f
              }
            else null
          assertArrayEquals(expected, detectCropV1(bitmap, false))
          assertNull(detectCropV1(bitmap, true))
        } finally {
          bitmap.recycle()
        }
      }
    }
  }

  @Test
  fun qrSkipDoesNotReadRecycledPixels() {
    val bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
    bitmap.recycle()
    assertNull(detectCropV1(bitmap, true))
  }
}
