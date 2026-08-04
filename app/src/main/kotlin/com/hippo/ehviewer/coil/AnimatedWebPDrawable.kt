package com.hippo.ehviewer.coil

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.drawable.Animatable
import android.graphics.drawable.Drawable
import android.os.SystemClock
import androidx.core.graphics.createBitmap
import com.ehviewer.core.util.logcat
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch

/**
 * Animated WebP drawable backed by a Rust-owned decoder and two reusable RGBA frame bitmaps.
 *
 * The decoder copies the bytes remaining in [source] during construction, so callers may release or
 * reuse their buffer afterward. Call [dispose] when the drawable leaves its owning image; the
 * operation is idempotent, and native destruction waits for any decode in flight before recycling
 * resources.
 */
class AnimatedWebPDrawable(source: ByteBuffer) : Drawable(), Animatable {
  private val decodeScope = CoroutineScope(Dispatchers.IO.limitedParallelism(1))
  private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.DITHER_FLAG or Paint.FILTER_BITMAP_FLAG)
  private val decoder: Long
  private val width: Int
  private val height: Int
  private val loopCount: Int
  private val disposed = AtomicBoolean(false)
  private val frameLock = Any()

  private var decodeJob: Job? = null
  private var loopsCompleted = 0
  private var timeToShowNextFrame = 0L
  private var currentFrame: Frame
  private var nextFrame: Frame

  init {
    val createdDecoder = nativeCreateDecoder(source.asDirectRemainingBuffer())
    check(createdDecoder > 0L) { "Failed to create decoder" }
    var firstBitmap: Bitmap? = null
    var secondBitmap: Bitmap? = null
    try {
      val packed = nativeGetImageInfo(createdDecoder)
      val decodedWidth = (packed shr 40).toInt()
      val decodedHeight = (packed shr 16 and 0xFFFFFF).toInt()
      val decodedLoopCount = (packed and 0xFFFF).toInt()
      require(decodedWidth > 0 && decodedHeight > 0) {
        "Animated WebP dimensions must be positive"
      }
      val first = createBitmap(decodedWidth, decodedHeight)
      firstBitmap = first
      val timestamp = nativeDecodeNextFrame(createdDecoder, false, first)
      check(timestamp != 0) { "Failed to decode first frame" }
      val second = createBitmap(decodedWidth, decodedHeight)
      secondBitmap = second

      decoder = createdDecoder
      width = decodedWidth
      height = decodedHeight
      loopCount = decodedLoopCount
      currentFrame = Frame(first, timestamp)
      nextFrame = Frame(second, 0)
      firstBitmap = null
      secondBitmap = null
    } catch (cause: Throwable) {
      firstBitmap?.recycle()
      secondBitmap?.recycle()
      nativeDestroyDecoder(createdDecoder)
      throw cause
    }
  }

  override fun getIntrinsicWidth() = width

  override fun getIntrinsicHeight() = height

  private val runnable = Runnable {
    synchronized(frameLock) {
      if (disposed.get()) return@Runnable
      timeToShowNextFrame =
        SystemClock.uptimeMillis() + nextFrame.timestamp - currentFrame.timestamp
      invalidateSelf()
    }
  }

  /** Creates, publishes, and then starts one decode job while [frameLock] is held. */
  private fun startDecodeLocked(reset: Boolean) {
    check(decodeJob == null) { "Animated WebP decode is already running" }
    lateinit var job: Job
    job =
      decodeScope
        .launch(start = CoroutineStart.LAZY) {
          val targetBitmap =
            synchronized(frameLock) {
              check(!disposed.get()) { "Animated WebP drawable is disposed" }
              nextFrame.bitmap
            }
          val timestamp = nativeDecodeNextFrame(decoder, reset, targetBitmap)
          ensureActive()
          synchronized(frameLock) {
            ensureActive()
            check(timestamp != 0) { "Failed to decode next frame" }
            if (timestamp <= currentFrame.timestamp) {
              if (reset) loopsCompleted = 0 else loopsCompleted++
              currentFrame.timestamp = 0
            }
            nextFrame.timestamp = timestamp
            nextFrame.bitmap.prepareToDraw()
          }
        }
        .apply {
          invokeOnCompletion { cause ->
            when {
              cause == null ->
                synchronized(frameLock) {
                  if (decodeJob === job && !disposed.get()) {
                    if (reset) {
                      runnable.run()
                    } else {
                      scheduleSelf(runnable, timeToShowNextFrame)
                    }
                  }
                }
              cause !is CancellationException -> {
                synchronized(frameLock) {
                  if (decodeJob === job) decodeJob = null
                }
                logcat(cause)
              }
            }
          }
        }
    decodeJob = job
    job.start()
  }

  override fun draw(canvas: Canvas) {
    synchronized(frameLock) {
      if (disposed.get()) return
      val completedJob = decodeJob
      if (completedJob?.isCompleted == true && isVisible) {
        decodeJob = null
        if (!completedJob.isCancelled && (loopCount == 0 || loopsCompleted < loopCount)) {
          currentFrame = nextFrame.also { nextFrame = currentFrame }
          startDecodeLocked(false)
        }
      }
      canvas.drawBitmap(currentFrame.bitmap, null, bounds, paint)
    }
  }

  @Deprecated("Deprecated in Java") override fun getOpacity(): Int = PixelFormat.TRANSLUCENT

  override fun setAlpha(alpha: Int) {
    if (alpha != paint.alpha) {
      paint.alpha = alpha
      invalidateSelf()
    }
  }

  override fun getAlpha(): Int = paint.alpha

  override fun setColorFilter(colorFilter: ColorFilter?) {
    if (colorFilter != paint.colorFilter) {
      paint.colorFilter = colorFilter
      invalidateSelf()
    }
  }

  override fun getColorFilter(): ColorFilter? = paint.colorFilter

  override fun isRunning() = synchronized(frameLock) { !disposed.get() && decodeJob != null }

  override fun start() {
    synchronized(frameLock) {
      if (!disposed.get() && decodeJob == null) {
        startDecodeLocked(true)
      }
    }
  }

  override fun stop() {
    synchronized(frameLock) {
      val job = decodeJob
      decodeJob = null
      job?.cancel()
      unscheduleSelf(runnable)
    }
  }

  /** Releases the native decoder and frame bitmaps exactly once. */
  fun dispose() {
    if (!disposed.compareAndSet(false, true)) return
    synchronized(frameLock) {
      decodeJob = null
      unscheduleSelf(runnable)
    }
    decodeScope.cancel()
    try {
      // Destroy takes the same Rust registry lock as frame decode, so it returns only after any
      // native bitmap write in flight has completed.
      nativeDestroyDecoder(decoder)
    } finally {
      synchronized(frameLock) {
        currentFrame.bitmap.recycle()
        nextFrame.bitmap.recycle()
      }
    }
  }
}

private class Frame(val bitmap: Bitmap, var timestamp: Int)

/** Copies [source] into a Rust-owned animated WebP decoder and returns its opaque handle. */
private external fun nativeCreateDecoder(source: ByteBuffer): Long

/** Returns packed width, height, and loop-count metadata for [decoder]. */
private external fun nativeGetImageInfo(decoder: Long): Long

/** Decodes one frame into [bitmap] and returns its cumulative timestamp in milliseconds. */
private external fun nativeDecodeNextFrame(decoder: Long, reset: Boolean, bitmap: Bitmap): Int

/** Idempotently releases the opaque Rust decoder handle. */
private external fun nativeDestroyDecoder(decoder: Long)

/**
 * Returns a direct buffer covering this buffer's remaining bytes without changing its position.
 *
 * Direct inputs produce a zero-copy slice. Heap inputs are copied because JNI direct-buffer access
 * cannot address heap-backed storage; Rust then synchronously takes its own lifetime-safe copy.
 */
internal fun ByteBuffer.asDirectRemainingBuffer(): ByteBuffer {
  val remaining = slice()
  if (remaining.isDirect) return remaining
  return ByteBuffer.allocateDirect(remaining.remaining()).apply {
    put(remaining)
    flip()
  }
}
