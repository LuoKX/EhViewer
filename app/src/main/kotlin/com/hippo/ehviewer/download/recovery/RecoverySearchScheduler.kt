package com.hippo.ehviewer.download.recovery

import android.os.SystemClock
import com.ehviewer.core.model.BaseGalleryInfo
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex

internal object RecoverySearchScheduler {
  private const val MIN_REQUEST_INTERVAL_MILLIS = 3_000L
  private val mutex = Mutex()
  private val cache = LinkedHashMap<String, List<BaseGalleryInfo>>()
  private var lastRequestAt = 0L

  suspend fun execute(
    normalizedKey: String,
    request: suspend () -> List<BaseGalleryInfo>,
  ): List<BaseGalleryInfo> {
    mutex.lock()
    return try {
      cache[normalizedKey]?.let { return it }
      val elapsed = SystemClock.elapsedRealtime() - lastRequestAt
      if (elapsed < MIN_REQUEST_INTERVAL_MILLIS) {
        delay(MIN_REQUEST_INTERVAL_MILLIS - elapsed)
      }
      lastRequestAt = SystemClock.elapsedRealtime()
      val result = request()
      cache[normalizedKey] = result
      while (cache.size > 128) cache.remove(cache.keys.first())
      result
    } finally {
      mutex.unlock()
    }
  }
}
