/*
 * Copyright 2022 Tarsin Norbin
 *
 * This file is part of EhViewer
 *
 * EhViewer is free software: you can redistribute it and/or
 * modify it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * EhViewer is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
 * or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with EhViewer.
 * If not, see <https://www.gnu.org/licenses/>.
 */
package com.hippo.ehviewer.image

import android.graphics.Bitmap
import android.hardware.HardwareBuffer
import android.os.Build
import androidx.compose.ui.unit.IntSize
import arrow.core.Either
import arrow.core.left
import arrow.core.right
import coil3.BitmapImage
import coil3.DrawableImage
import coil3.Image as CoilImage
import coil3.request.CachePolicy
import coil3.request.ErrorResult
import coil3.request.SuccessResult
import coil3.request.allowHardware
import coil3.request.maxBitmapSize
import coil3.size.Precision
import coil3.size.Scale
import coil3.size.Size
import coil3.size.SizeResolver
import com.ehviewer.core.files.openFileDescriptor
import com.ehviewer.core.files.toUri
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.coil.AnimatedWebPDrawable
import com.hippo.ehviewer.coil.BitmapImageWithExtraInfo
import com.hippo.ehviewer.coil.detectQrCode
import com.hippo.ehviewer.coil.hardwareThreshold
import com.hippo.ehviewer.coil.maybeCropBorder
import com.hippo.ehviewer.jni.prepareGifSourceOwned
import com.hippo.ehviewer.jni.rewriteGifSource
import com.hippo.ehviewer.jni.shouldUseGifWorkaround
import com.hippo.ehviewer.ktbuilder.execute
import com.hippo.ehviewer.ktbuilder.imageRequest
import java.nio.ByteBuffer
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.atomics.AtomicReference
import kotlin.concurrent.atomics.decrementAndFetch
import kotlin.concurrent.atomics.updateAndFetch
import okio.Path
import splitties.init.appCtx

class Image private constructor(image: CoilImage, src: ImageSource) {
  val refcnt = AtomicInt(1)

  fun pin() = refcnt.updateAndFetch { if (it != 0) it + 1 else 0 } != 0

  fun unpin() = (refcnt.decrementAndFetch() == 0).also { if (it) recycle() }

  val intrinsicSize = with(image) { IntSize(width, height) }
  val hasQrCode =
    when (image) {
      is BitmapImageWithExtraInfo -> image.hasQrCode
      else -> false
    }

  var innerImage: CoilImage? =
    when (image) {
      is BitmapImageWithExtraInfo -> image.image
      else -> image
    }

  // Static bitmaps and our WebP decoder own their pixels/input after decoding. Keeping the
  // closed source here would also keep archive extraction buffers alive for the whole cache entry.
  private var src: ImageSource? =
    if (
      innerImage is BitmapImage || (innerImage as? DrawableImage)?.drawable is AnimatedWebPDrawable
    ) {
      src.close()
      null
    } else {
      src
    }

  val allocationSize =
    image.size + ((this.src as? ByteBufferSource)?.source?.capacity()?.toLong() ?: 0L)

  private fun recycle() {
    try {
      when (val image = innerImage!!) {
        is DrawableImage -> (image.drawable as? AnimatedWebPDrawable)?.dispose()
        is BitmapImage -> image.bitmap.recycle()
      }
    } finally {
      innerImage = null
      val source = src
      src = null
      source?.close()
    }
  }

  companion object {
    private val sizeResolver =
      with(appCtx.resources.displayMetrics) {
        val targetSize = minOf(widthPixels, heightPixels) * 4 / 3
        SizeResolver(Size(targetSize, targetSize))
      }

    private suspend fun Either<ByteBufferSource, PathSource>.decodeCoil(
      checkExtraneousAds: Boolean
    ): CoilImage {
      val request =
        with(appCtx) {
          imageRequest {
            onLeft { data(it.source) }
            onRight { data(it.source.toUri()) }
            size(sizeResolver)
            scale(Scale.FILL)
            precision(Precision.INEXACT)
            maxBitmapSize(Size.ORIGINAL)
            allowHardware(false)
            hardwareThreshold(Settings.hardwareBitmapThreshold.value)
            maybeCropBorder(Settings.cropBorder.value)
            detectQrCode(checkExtraneousAds)
            memoryCachePolicy(CachePolicy.DISABLED)
          }
        }
      return when (val result = request.execute()) {
        is SuccessResult -> result.image
        is ErrorResult -> throw result.throwable
      }
    }

    private suspend fun decodePreparedGif(
      source: ByteArray,
      checkExtraneousAds: Boolean,
    ): Image {
      val prepared = byteBufferSource(ByteBuffer.wrap(source)) {}
      val image =
        try {
          prepared.left().decodeCoil(checkExtraneousAds)
        } catch (failure: Throwable) {
          prepared.close()
          throw failure
        }
      return Image(image, prepared)
    }

    suspend fun decode(src: ImageSource, checkExtraneousAds: Boolean = false): Image {
      val useGifWorkaround = shouldUseGifWorkaround(Build.VERSION.SDK_INT)
      val image =
        when (src) {
          is PathSource -> {
            if (useGifWorkaround) {
              val prepared =
                src.source.openFileDescriptor("r").use {
                  prepareGifSourceOwned(it.fd)
                }
              if (prepared != null) {
                src.close()
                return decodePreparedGif(prepared, checkExtraneousAds)
              }
            }
            src.right().decodeCoil(checkExtraneousAds)
          }
          is ByteBufferSource -> {
            if (useGifWorkaround) {
              rewriteGifSource(src.source)
            }
            src.left().decodeCoil(checkExtraneousAds)
          }
        }
      return Image(image, src)
    }
  }
}

sealed interface ImageSource : AutoCloseable

interface PathSource : ImageSource {
  val source: Path
  val type: String
}

interface ByteBufferSource : ImageSource {
  val source: ByteBuffer
}

inline fun byteBufferSource(buffer: ByteBuffer, crossinline release: () -> Unit) =
  object : ByteBufferSource {
    private val buffer = AtomicReference<ByteBuffer?>(buffer)

    override val source: ByteBuffer
      get() = checkNotNull(this.buffer.load()) { "Image source is closed" }

    override fun close() {
      if (this.buffer.exchange(null) != null) release()
    }
  }

/** Returns the content bounds detected by the host-tested Rust border algorithm. */
external fun detectBorder(bitmap: Bitmap): IntArray

/** Returns an accepted Rust crop, or null when the original image must be kept. */
external fun detectCropV1(bitmap: Bitmap, hasQrCode: Boolean): IntArray?

/** Returns whether the host-tested Rust finder-pattern algorithm detects a QR code. */
external fun hasQrCode(bitmap: Bitmap): Boolean

/**
 * Copies the [dst] sized region of [src] at ([x], [y]) into the hardware-buffer origin.
 *
 * Source and destination packed-pixel formats must match. Invalid coordinates, layouts, formats, or
 * native locks throw a contained runtime exception so callers can use their software fallback.
 */
external fun copyBitmapToAHB(src: Bitmap, dst: HardwareBuffer, x: Int, y: Int)
