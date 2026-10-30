package com.hippo.ehviewer.ui.legacy

import android.graphics.drawable.DrawableWrapper
import android.text.Html
import coil3.asDrawable
import coil3.decode.DecodeUtils
import coil3.imageLoader
import coil3.request.crossfade
import coil3.size.Scale
import coil3.size.Size
import com.hippo.ehviewer.bridge.gallery.commentImageDimensions
import com.hippo.ehviewer.ktbuilder.imageRequest
import kotlin.math.roundToInt
import splitties.init.appCtx

class CoilImageGetter(private val onSuccess: () -> Unit) : Html.ImageGetter {
  override fun getDrawable(source: String) =
    object : DrawableWrapper(null) {}.apply {
      commentImageDimensions(source)?.let {
        val (srcWidth, srcHeight, dstWidth, dstHeight) = it
        val multiplier =
          DecodeUtils.computeSizeMultiplier(
            srcWidth,
            srcHeight,
            dstWidth,
            dstHeight,
            Scale.FIT,
            Size.ORIGINAL,
          )
        setBounds(0, 0, (srcWidth * multiplier).roundToInt(), (srcHeight * multiplier).roundToInt())
      } ?: setBounds(0, 0, 200, 300)
      with(appCtx) {
        imageLoader.enqueue(
          imageRequest {
            data(source)
            crossfade(false)
            target { image ->
              setDrawable(image.asDrawable(resources))
              onSuccess()
            }
          }
        )
      }
    }
}
