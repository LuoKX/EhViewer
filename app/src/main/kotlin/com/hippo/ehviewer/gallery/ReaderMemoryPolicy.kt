package com.hippo.ehviewer.gallery

import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

private const val MAX_IMAGE_CACHE_SIZE = 128L * 1024 * 1024

/**
 * Leave heap space for extraction buffers, concurrent decoders, thumbnails, and the rest of the UI.
 */
internal fun readerImageCacheSize(appMaxMemory: Long, totalMemory: Long): Int {
  val heapBudget = (appMaxMemory / 4).coerceAtLeast(1L)
  val deviceBudget = if (totalMemory > 0) totalMemory / 8 else heapBudget
  return minOf(heapBudget, deviceBudget, MAX_IMAGE_CACHE_SIZE).coerceAtLeast(1L).toInt()
}

/**
 * Keep one oversized page alive until the UI pins it, without overflowing SieveCache's Int size.
 */
internal fun readerImageCacheEntrySize(allocationSize: Long, cacheSize: Int): Int =
  allocationSize.coerceIn(1L, cacheSize.toLong()).toInt()

/** Reopen/redecode once after releasing idle images; never retry cancellation or loop on OOM. */
internal suspend inline fun <T> retryAfterImageCacheTrim(trimCache: () -> Unit, block: () -> T): T {
  try {
    return block()
  } catch (_: OutOfMemoryError) {
    trimCache()
  }
  currentCoroutineContext().ensureActive()
  return block()
}
