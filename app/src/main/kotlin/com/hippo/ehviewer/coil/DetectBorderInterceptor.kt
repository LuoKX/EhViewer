package com.hippo.ehviewer.coil

import androidx.compose.ui.unit.IntRect
import coil3.Extras
import coil3.getExtra
import coil3.intercept.Interceptor
import coil3.intercept.Interceptor.Chain
import coil3.request.ImageRequest
import coil3.request.ImageResult
import coil3.request.SuccessResult
import com.hippo.ehviewer.image.detectCropV1

private val maybeCropBorderKey = Extras.Key(default = false)

fun ImageRequest.Builder.maybeCropBorder(enable: Boolean) = apply {
  extras[maybeCropBorderKey] = enable
}

val ImageRequest.maybeCropBorder: Boolean
  get() = getExtra(maybeCropBorderKey)

object DetectBorderInterceptor : Interceptor {
  override suspend fun intercept(chain: Chain): ImageResult {
    val result = chain.proceed()
    if (chain.request.maybeCropBorder && result is SuccessResult) {
      val image = result.image
      if (image is BitmapImageWithExtraInfo) {
        val array = detectCropV1(image.image.bitmap, image.hasQrCode) ?: return result
        require(
          array.size == 4 &&
            array[0] >= 0 &&
            array[1] >= 0 &&
            array[2] in array[0]..image.width &&
            array[3] in array[1]..image.height
        ) {
          "Invalid Rust crop rectangle"
        }
        return result.copy(
          image = image.copy(rect = IntRect(array[0], array[1], array[2], array[3]))
        )
      }
    }
    return result
  }
}
