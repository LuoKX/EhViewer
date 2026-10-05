package com.hippo.ehviewer.download.recovery

import android.os.SystemClock
import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.bridge.download.RecoverySearchRequest
import com.hippo.ehviewer.bridge.download.planRecoverySearch
import kotlinx.coroutines.delay
import kotlinx.coroutines.sync.Mutex

internal object RecoverySearchScheduler {
  private val mutex = Mutex()
  private val cache = LinkedHashMap<String, List<BaseGalleryInfo>>()
  private var lastRequestAt = 0L

  suspend fun execute(
    normalizedKey: String,
    request: suspend () -> List<BaseGalleryInfo>,
  ): List<BaseGalleryInfo> {
    mutex.lock()
    return try {
      val cached = cache[normalizedKey]
      val before =
        planRecoverySearch(
          RecoverySearchRequest(
            cached = cached != null,
            elapsedMillis = SystemClock.elapsedRealtime() - lastRequestAt,
            cacheSize = cache.size,
          )
        )
      if (before.useCache) return checkNotNull(cached)
      delay(before.waitMillis)
      lastRequestAt = SystemClock.elapsedRealtime()
      val result = request()
      val after =
        planRecoverySearch(
          RecoverySearchRequest(
            cached = false,
            elapsedMillis = 0,
            cacheSize = cache.size,
            hasResults = result.isNotEmpty(),
          )
        )
      if (after.store) {
        cache[normalizedKey] = result
        repeat(after.evictCount) { cache.remove(cache.keys.first()) }
      }
      result
    } finally {
      mutex.unlock()
    }
  }
}
