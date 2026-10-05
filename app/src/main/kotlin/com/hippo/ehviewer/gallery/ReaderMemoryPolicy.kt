package com.hippo.ehviewer.gallery

import com.hippo.ehviewer.bridge.reader.readerImageEntrySize
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

/** Scalar Rust accounting keeps one oversized page alive without Int overflow. */
internal fun readerImageCacheEntrySize(allocationSize: Long, cacheSize: Int): Int =
  readerImageEntrySize(allocationSize, cacheSize)

/** Applies a prevalidated Rust retry budget; OOM handling performs no JNI allocation. */
internal suspend inline fun <T> retryAfterImageCacheTrim(
  attempts: Int,
  trimCache: () -> Unit,
  block: () -> T,
): T {
  require(attempts in 1..2) { "Invalid reader retry budget" }
  repeat(attempts) { attempt ->
    try {
      return block()
    } catch (failure: OutOfMemoryError) {
      if (attempt == attempts - 1) throw failure
      trimCache()
    }
    currentCoroutineContext().ensureActive()
  }
  error("Unreachable reader retry state")
}
