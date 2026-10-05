package com.hippo.ehviewer.gallery

import androidx.collection.SieveCache
import androidx.collection.mutableIntObjectMapOf
import arrow.fx.coroutines.ExitCase
import arrow.fx.coroutines.bracketCase
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.util.withNonCancellableContext
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.reader.ReaderPageState
import com.hippo.ehviewer.bridge.reader.ReaderPolicyKind
import com.hippo.ehviewer.bridge.reader.ReaderPolicyRequest
import com.hippo.ehviewer.bridge.reader.ReaderScheduleActionKind
import com.hippo.ehviewer.bridge.reader.ReaderScheduleCommand
import com.hippo.ehviewer.bridge.reader.ReaderSchedulePlan
import com.hippo.ehviewer.bridge.reader.ReaderScheduleSnapshot
import com.hippo.ehviewer.bridge.reader.planReaderPolicy
import com.hippo.ehviewer.bridge.reader.planReaderSchedule
import com.hippo.ehviewer.image.Image
import com.hippo.ehviewer.image.ImageSource
import com.hippo.ehviewer.util.FileUtils
import com.hippo.ehviewer.util.OSUtils
import com.hippo.ehviewer.util.detectAds
import com.hippo.ehviewer.util.displayString
import java.util.concurrent.locks.ReentrantLock
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.read
import kotlin.concurrent.withLock as withJavaLock
import kotlin.concurrent.write
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import moe.tarsin.coroutines.NamedMutex
import moe.tarsin.coroutines.withLock
import okio.Path

private val progressScope = CoroutineScope(Dispatchers.IO)

abstract class PageLoader(
  val scope: CoroutineScope,
  val info: GalleryInfo?,
  startPage: Int,
  val size: Int,
  val hasAds: Boolean = false,
) : AutoCloseable {
  var startPage = startPage.coerceIn(0, size - 1)

  private val jobs = mutableIntObjectMapOf<Job>()
  private val mutex = NamedMutex<Int>()
  private val memoryPlan =
    planReaderPolicy(
      ReaderPolicyRequest(
        kind = ReaderPolicyKind.MEMORY,
        appMaxMemory = OSUtils.appMaxMemory,
        totalMemory = OSUtils.totalMemory,
      )
    )
  private val semaphore = Semaphore(checkNotNull(memoryPlan.decodeConcurrency))
  private val cacheSize = checkNotNull(memoryPlan.cacheSize)

  private val cache =
    SieveCache<Int, Image>(
      maxSize = cacheSize,
      // A single oversized page must survive until the UI can pin it. Let it occupy the whole
      // cache instead of immediately evicting/recycling the image we are about to publish.
      sizeOf = { _, v -> readerImageCacheEntrySize(v.allocationSize, cacheSize) },
      onEntryRemoved = { k, o, n, _ -> if (o.unpin()) n ?: notifyPageWait(k) },
    )

  private suspend fun atomicallyDecodeAndUpdate(index: Int) {
    bracketCase(
      { openSource(index) },
      { src ->
        withNonCancellableContext {
          notifyPageSucceed(index, Image.decode(src, hasAds && detectAds(index, size)))
        }
      },
      { src, case -> if (case !is ExitCase.Completed) src.close() },
    )
  }

  private val lock = ReentrantReadWriteLock()
  private val scheduleLock = ReentrantLock()

  val pages = (0 until size).map { Page(it) }

  private val prefetchPageCount = Settings.preloadImage.value

  fun restart() {
    scheduleLock.withJavaLock {
      applyReaderSchedulePlan(
        planReaderSchedule(readerScheduleSnapshot(), ReaderScheduleCommand.restart())
      )
    }
  }

  private val prevIndex = AtomicInt(-1)

  fun retryPage(index: Int, orgImg: Boolean = false) {
    scheduleLock.withJavaLock {
      applyReaderSchedulePlan(
        planReaderSchedule(
          readerScheduleSnapshot(),
          ReaderScheduleCommand.retry(index, orgImg),
        )
      )
    }
  }

  protected abstract fun prefetchPages(pages: List<Int>, bounds: IntRange)

  protected abstract fun onRequest(index: Int, force: Boolean = false, orgImg: Boolean = false)

  fun notifyPageWait(index: Int) {
    pages[index].reset()
  }

  fun notifyPagePercent(index: Int, percent: Float) {
    pages[index].statusFlow.update {
      when (it) {
        is PageStatus.Loading -> it.apply { progress.update { percent } }
        else -> PageStatus.Loading(MutableStateFlow(percent))
      }
    }
  }

  fun notifyPageSucceed(index: Int, image: Image, replaceCache: Boolean = true) {
    if (replaceCache) lock.write { cache[index] = image }
    pages[index].statusFlow.update {
      if (image.hasQrCode) PageStatus.Blocked(image) else PageStatus.Ready(image)
    }
  }

  fun notifyPageFailed(index: Int, error: String?) {
    pages[index].statusFlow.update { PageStatus.Error(error) }
  }

  override fun close() {
    lock.write { cache.evictAll() }
    info?.run {
      progressScope.launch {
        EhDB.putReadProgress(gid, startPage)
      }
    }
  }

  abstract val title: String

  protected abstract fun getImageExtension(index: Int): String?

  fun getImageFilename(index: Int): String? =
    getImageExtension(index)?.let {
      FileUtils.sanitizeFilename("$title - ${index + 1}.${it.lowercase()}")
    }

  fun request(index: Int) {
    scheduleLock.withJavaLock { requestWithRust(index) }
  }

  private fun requestWithRust(index: Int) {
    val cachedImage = lock.read { cache[index]?.takeIf(Image::pin) }
    try {
      val plan =
        planReaderSchedule(
          readerScheduleSnapshot(),
          ReaderScheduleCommand.request(
            index = index,
            cached = cachedImage != null,
            prefetchPageCount = prefetchPageCount,
          ),
        )
      applyReaderSchedulePlan(plan, cachedImage)
    } finally {
      cachedImage?.unpin()
    }
  }

  private fun readerScheduleSnapshot() =
    ReaderScheduleSnapshot(
      pageStates =
        pages.map { page ->
          when (page.status) {
            PageStatus.Queued -> ReaderPageState.QUEUED
            is PageStatus.Loading -> ReaderPageState.LOADING
            is PageStatus.Blocked -> ReaderPageState.BLOCKED
            is PageStatus.Ready -> ReaderPageState.READY
            is PageStatus.Error -> ReaderPageState.ERROR
          }
        },
      previousIndex = prevIndex.load().takeIf { it >= 0 },
    )

  private fun applyReaderSchedulePlan(plan: ReaderSchedulePlan, cachedImage: Image? = null) {
    plan.actions.forEach { action ->
      when (action.kind) {
        ReaderScheduleActionKind.SET_PREVIOUS -> prevIndex.store(checkNotNull(action.index))
        ReaderScheduleActionKind.RESET_ONE -> notifyPageWait(checkNotNull(action.index))
        ReaderScheduleActionKind.USE_CACHED ->
          notifyPageSucceed(checkNotNull(action.index), checkNotNull(cachedImage), false)
        ReaderScheduleActionKind.RESET_ALL -> pages.forEach(Page::reset)
        ReaderScheduleActionKind.REMOVE_CACHE ->
          lock.write { cache.remove(checkNotNull(action.index)) }
        ReaderScheduleActionKind.CLEAR_CACHE -> lock.write { cache.evictAll() }
        ReaderScheduleActionKind.FETCH ->
          onRequest(
            checkNotNull(action.index),
            checkNotNull(action.force),
            checkNotNull(action.originalImage),
          )
        ReaderScheduleActionKind.PREFETCH ->
          prefetchPages(
            checkNotNull(action.pages),
            checkNotNull(action.boundsStart)..checkNotNull(action.boundsEnd),
          )
      }
    }
  }

  fun cancelRequest(index: Int) {
    jobs[index]?.cancel()
  }

  abstract fun save(index: Int, file: Path): Boolean

  fun notifySourceReady(index: Int) =
    synchronized(jobs) {
      if (jobs[index]?.isActive != true) {
        jobs[index] = scope.launch {
          try {
            mutex.withLock(index) {
              semaphore.withPermit {
                retryAfterImageCacheTrim(
                  attempts = checkNotNull(memoryPlan.decodeAttempts),
                  trimCache = {
                    lock.write {
                      // Keep pages currently pinned by the reader; release only idle cache entries.
                      cache.removeIf { _, image -> image.refcnt.load() == 1 }
                    }
                  },
                ) {
                  atomicallyDecodeAndUpdate(index)
                }
              }
            }
          } catch (e: Throwable) {
            if (e is CancellationException) {
              notifyPageFailed(index, null)
              throw e
            } else {
              notifyPageFailed(index, e.displayString())
            }
          }
        }
      }
    }

  abstract fun openSource(index: Int): ImageSource
}
