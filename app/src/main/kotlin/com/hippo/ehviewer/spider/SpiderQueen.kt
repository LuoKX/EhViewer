/*
 * Copyright 2016 Hippo Seven
 * Rewrite with Kotlin coroutines, Tarsin Norbin 2023
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.spider

import androidx.annotation.IntDef
import arrow.core.Either
import arrow.core.getOrElse
import arrow.core.partially1
import com.ehviewer.core.files.find
import com.ehviewer.core.i18n.R
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.model.GalleryPreview
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.reader.ReaderFetchActionKind
import com.hippo.ehviewer.bridge.reader.ReaderFetchErrorSource
import com.hippo.ehviewer.bridge.reader.ReaderFetchEvent
import com.hippo.ehviewer.bridge.reader.ReaderFetchImageFailure
import com.hippo.ehviewer.bridge.reader.ReaderFetchImageTarget
import com.hippo.ehviewer.bridge.reader.ReaderFetchPageSource
import com.hippo.ehviewer.bridge.reader.ReaderFetchPhase
import com.hippo.ehviewer.bridge.reader.ReaderFetchState
import com.hippo.ehviewer.bridge.reader.consumePreappliedImageCleanup
import com.hippo.ehviewer.bridge.reader.startReaderFetchPlanner
import com.hippo.ehviewer.client.EhEngine
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.EhUrl.getGalleryDetailUrl
import com.hippo.ehviewer.client.EhUrl.referer
import com.hippo.ehviewer.client.EhUtils
import com.hippo.ehviewer.client.exception.FatalException
import com.hippo.ehviewer.client.exception.QuotaExceededException
import com.hippo.ehviewer.util.displayString
import kotlin.concurrent.atomics.AtomicInt
import kotlin.concurrent.atomics.decrementAndFetch
import kotlin.concurrent.atomics.incrementAndFetch
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.TimeSource
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.sync.withPermit
import moe.tarsin.coroutines.runSuspendCatching
import okio.FileNotFoundException
import okio.Path
import splitties.init.appCtx

class SpiderQueen private constructor(val galleryInfo: GalleryInfo) : CoroutineScope {
  override val coroutineContext = Dispatchers.IO + Job()

  @Volatile lateinit var pageStates: IntArray
  lateinit var spiderInfo: SpiderInfo

  val spiderDen: SpiderDen = SpiderDen(galleryInfo)
  private val mPageStateLock = Any()
  private val mDownloadedPages = AtomicInt(0)
  private val mFinishedPages = AtomicInt(0)
  private val mSpiderListeners: MutableList<OnSpiderListener> = ArrayList()

  private var mReadReference = 0
  private var mDownloadReference = 0

  fun addOnSpiderListener(listener: OnSpiderListener) {
    synchronized(mSpiderListeners) { mSpiderListeners.add(listener) }
  }

  fun removeOnSpiderListener(listener: OnSpiderListener) {
    synchronized(mSpiderListeners) { mSpiderListeners.remove(listener) }
  }

  private fun notifyGetPages(pages: Int) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach { it.onGetPages(pages) }
    }
  }

  fun notifyFatal(error: FatalException) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach { it.onFatal(error) }
    }
  }

  fun notifyPageDownload(index: Int, contentLength: Long, receivedSize: Long, bytesRead: Int) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach {
        it.onPageDownload(
          index,
          contentLength,
          receivedSize,
          bytesRead,
        )
      }
    }
  }

  private fun notifyPageSuccess(index: Int) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach {
        it.onPageSuccess(
          index,
          mFinishedPages.load(),
          mDownloadedPages.load(),
          pageStates.size,
        )
      }
    }
  }

  private fun notifyPageFailure(index: Int, error: String?) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach {
        it.onPageFailure(
          index,
          error,
          mFinishedPages.load(),
          mDownloadedPages.load(),
          pageStates.size,
        )
      }
    }
  }

  private fun notifyAllPageDownloaded(failed: Boolean) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach {
        it.onFinish(
          mFinishedPages.load(),
          mDownloadedPages.load(),
          pageStates.size,
          failed,
        )
      }
    }
  }

  private fun notifyPageReady(index: Int) {
    synchronized(mSpiderListeners) {
      mSpiderListeners.forEach {
        it.onPageReady(index)
      }
    }
  }

  private var downloadMode = false

  private val isReady
    get() = this::spiderInfo.isInitialized && this::pageStates.isInitialized

  private val updateLock = Mutex()

  private suspend fun updateMode() = runSuspendCatching {
    awaitReady()
    updateLock.withLock {
      val mode: Int =
        if (mDownloadReference > 0) {
          MODE_DOWNLOAD
        } else {
          MODE_READ
        }
      spiderDen.setMode(mode)

      // Update download page
      val intoDownloadMode = mode == MODE_DOWNLOAD
      if (intoDownloadMode && !downloadMode) {
        // Clear download state
        synchronized(mPageStateLock) {
          val temp: IntArray = pageStates
          var i = 0
          val n = temp.size
          while (i < n) {
            val oldState = temp[i]
            if (STATE_DOWNLOADING != oldState) {
              temp[i] = STATE_NONE
            }
            i++
          }
          mDownloadedPages.store(0)
          mFinishedPages.store(0)
        }
        mWorkerScope.enterDownloadMode()
      }
      downloadMode = intoDownloadMode
    }
  }

  private fun setMode(@Mode mode: Int) {
    when (mode) {
      MODE_READ -> mReadReference++
      MODE_DOWNLOAD -> mDownloadReference++
    }
    check(mDownloadReference <= 1) { "mDownloadReference can't more than 1" }
  }

  private fun clearMode(@Mode mode: Int) {
    when (mode) {
      MODE_READ -> mReadReference--
      MODE_DOWNLOAD -> mDownloadReference--
    }
    check(!(mReadReference < 0 || mDownloadReference < 0)) { "Mode reference < 0" }
  }

  private val prepareScope = CoroutineScope(coroutineContext + SupervisorJob())
  private val prepareJob = prepareScope.async { doPrepare() }
  private val archiveJob =
    launch(start = CoroutineStart.LAZY) {
      val failed = spiderDen.archive()
      notifyAllPageDownloaded(failed)
    }

  private suspend fun doPrepare() {
    spiderDen.initDownloadDirIfExist()
    val pages =
      Either.catch {
          spiderInfo = readSpiderInfoFromLocal() ?: readSpiderInfoFromInternet()
          spiderInfo.pages
        }
        .getOrElse {
          logcat(it)
          galleryInfo.pages
        }
    check(pages > 0)
    pageStates = IntArray(pages)
    notifyGetPages(pages)
  }

  suspend fun awaitReady() = prepareJob.await()

  private fun stop() {
    val queenScope = this
    launch {
      if (archiveJob.isActive) {
        archiveJob.join()
      }
      if (!spiderDen.postArchive()) {
        if (mWorkerScope.isDownloadMode) {
          runCatching {
            spiderDen.writeComicInfo()
          }
            .onFailure {
              logcat(it)
            }
        }
        runCatching {
          writeSpiderInfoToLocal()
        }
          .onFailure {
            logcat(it)
          }
      }
      queenScope.cancel()
    }
  }

  val size
    get() = pageStates.size

  val error: String?
    get() = null

  private fun getPageState(index: Int): Int {
    synchronized(mPageStateLock) {
      return if (index >= 0 && index < pageStates.size) {
        pageStates[index]
      } else {
        STATE_NONE
      }
    }
  }

  fun preloadPages(pages: List<Int>, pair: IntRange) {
    mWorkerScope.updateRAList(pages, pair)
  }

  fun request(index: Int, force: Boolean, orgImg: Boolean = false) {
    // Get page state
    val state = getPageState(index)

    // Fix state for force
    if (force && state == STATE_FINISHED || state == STATE_FAILED) {
      // Update state to none at once
      updatePageState(index, STATE_NONE)
    }
    mWorkerScope.launch(index, force, orgImg)
  }

  fun save(index: Int, file: Path): Boolean {
    val state = getPageState(index)
    return if (STATE_FINISHED != state) {
      false
    } else {
      spiderDen.saveToPath(index, file)
    }
  }

  fun getExtension(index: Int): String? {
    val state = getPageState(index)
    return if (STATE_FINISHED != state) {
      null
    } else {
      spiderDen.getExtension(index)
    }
  }

  private fun readSpiderInfoFromLocal(): SpiderInfo? =
    spiderDen.downloadDir?.run {
      find(SPIDER_INFO_FILENAME)?.let { file ->
        readCompatFromPath(file)?.takeIf {
          it.gid == galleryInfo.gid && it.token == galleryInfo.token
        }
      }
    }
      ?: readFromCache(galleryInfo.gid)?.takeIf {
        it.gid == galleryInfo.gid && it.token == galleryInfo.token
      }

  private fun readPreviews(previews: List<GalleryPreview>, index: Int, spiderInfo: SpiderInfo) {
    if (index == 0) {
      spiderInfo.previewPerPage = previews.size
    } else {
      spiderInfo.previewPerPage = previews[0].position / index
    }
    previews.forEach {
      if (it.pToken.isNotEmpty()) {
        spiderInfo.pTokenMap[it.position] = it.pToken
      }
    }
  }

  private suspend fun readSpiderInfoFromInternet() =
    EhEngine.getPreviewList(getGalleryDetailUrl(galleryInfo.gid, galleryInfo.token)).run {
      val spiderInfo = SpiderInfo(galleryInfo.gid, galleryInfo.token, total)
      readPreviews(previews, 0, spiderInfo)
      spiderInfo
    }

  private val isMpvAvailable = EhUtils.isMpvAvailable

  suspend fun getPTokenFromMultiPageViewer(index: Int): String? {
    if (!isMpvAvailable) return null
    return runSuspendCatching {
      EhEngine.getPTokenListFromMpv(galleryInfo.gid, galleryInfo.token).forEachIndexed { index, s ->
        spiderInfo.pTokenMap[index] = s
      }
      spiderInfo.pTokenMap[index]
    }
      .getOrElse {
        logcat(it)
        null
      }
  }

  suspend fun getPTokenFromInternet(index: Int): String? {
    val previewIndex =
      if (spiderInfo.previewPerPage > 0) {
        index / spiderInfo.previewPerPage
      } else {
        0
      }
    val url = getGalleryDetailUrl(galleryInfo.gid, galleryInfo.token, previewIndex)
    return runSuspendCatching {
      EhEngine.getPreviewList(url).run {
        readPreviews(previews, previewIndex, spiderInfo)
        spiderInfo.pTokenMap[index]
      }
    }
      .getOrElse {
        logcat(it)
        null
      }
  }

  @Synchronized
  private fun writeSpiderInfoToLocal() {
    if (!isReady) return
    spiderDen.downloadDir?.run { spiderInfo.write(resolve(SPIDER_INFO_FILENAME)) }
    spiderInfo.saveToCache()
  }

  private fun isStateDone(state: Int): Boolean = state == STATE_FINISHED || state == STATE_FAILED

  fun updatePageState(index: Int, @State state: Int, error: String? = null) {
    synchronized<Unit>(mPageStateLock) {
      val oldState = pageStates[index]
      pageStates[index] = state
      if (!isStateDone(oldState) && isStateDone(state)) {
        mDownloadedPages.incrementAndFetch()
      } else if (isStateDone(oldState) && !isStateDone(state)) {
        mDownloadedPages.decrementAndFetch()
      }
      if (oldState != STATE_FINISHED && state == STATE_FINISHED) {
        mFinishedPages.incrementAndFetch()
      } else if (oldState == STATE_FINISHED && state != STATE_FINISHED) {
        mFinishedPages.decrementAndFetch()
      }
    }

    // Notify listeners
    if (state == STATE_FAILED) {
      notifyPageFailure(index, error)
    } else if (state == STATE_FINISHED) {
      notifyPageSuccess(index)
    }
    if (mDownloadedPages.load() == size) {
      if (mFinishedPages.load() == size) {
        archiveJob.start()
      } else {
        notifyAllPageDownloaded(failed = true)
      }
    }
  }

  @IntDef(MODE_READ, MODE_DOWNLOAD) @Retention annotation class Mode

  @IntDef(STATE_NONE, STATE_DOWNLOADING, STATE_FINISHED, STATE_FAILED)
  @Retention(AnnotationRetention.SOURCE)
  annotation class State

  interface OnSpiderListener {
    fun onGetPages(pages: Int) {}

    fun onFatal(error: FatalException) {}

    fun onPageDownload(index: Int, contentLength: Long, receivedSize: Long, bytesRead: Int) {}

    fun onPageSuccess(index: Int, finished: Int, downloaded: Int, total: Int) {}

    fun onPageFailure(index: Int, error: String?, finished: Int, downloaded: Int, total: Int) {}

    fun onFinish(finished: Int, downloaded: Int, total: Int, failed: Boolean) {}

    fun onPageReady(index: Int) {}
  }

  companion object {
    const val MODE_READ = 0
    const val MODE_DOWNLOAD = 1
    const val STATE_NONE = 0
    const val STATE_DOWNLOADING = 1
    const val STATE_FINISHED = 2
    const val STATE_FAILED = 3
    const val SPIDER_INFO_FILENAME = ".ehviewer"
    private val sQueenMap = mutableMapOf<Long, SpiderQueen>()

    fun obtainSpiderQueen(galleryInfo: GalleryInfo, @Mode mode: Int): SpiderQueen {
      val gid = galleryInfo.gid
      return (sQueenMap.getOrPut(gid) { SpiderQueen(galleryInfo) }).apply {
        setMode(mode)
        launch { updateMode() }
      }
    }

    fun releaseSpiderQueen(queen: SpiderQueen, @Mode mode: Int) {
      queen.run {
        clearMode(mode)
        if (mReadReference == 0 && mDownloadReference == 0) {
          stop()
          sQueenMap.remove(galleryInfo.gid)
        } else {
          launch { updateMode() }
        }
      }
    }
  }

  private val mWorkerScope =
    object {
      private val jobs = hashMapOf<Int, Job>()
      private val semaphore = Semaphore(Settings.multiThreadDownload.value)
      private val pTokenLock = Mutex()
      private var showKey: String? = null
      private val showKeyLock = Mutex()
      private val downloadDelay = Settings.downloadDelay.value.milliseconds
      private var lastRequestTime = TimeSource.Monotonic.markNow()
      var isDownloadMode = false
        private set

      @Synchronized
      fun enterDownloadMode() {
        if (isDownloadMode) return
        updateRAList((0 until size).toList())
        isDownloadMode = true
      }

      fun updateRAList(list: List<Int>, aliveBound: IntRange = 0..Int.MAX_VALUE) {
        if (isDownloadMode) return
        synchronized(jobs) {
          jobs.forEach { (i, job) ->
            if (i !in aliveBound) {
              job.cancel()
            }
          }
          list.forEach {
            if (pageStates[it] != STATE_FINISHED && jobs[it]?.isActive != true) {
              doLaunchDownloadJob(it, false)
            }
          }
        }
      }

      private fun doLaunchDownloadJob(index: Int, force: Boolean, orgImg: Boolean = false) {
        val currentJob = jobs[index]
        val skipHath = force && !orgImg && currentJob?.isActive == true
        if (force) currentJob?.cancel(CancellationException(FORCE_RETRY))
        if (currentJob?.isActive != true) {
          jobs[index] = launch {
            runCatching {
              semaphore.withPermit {
                doInJob(index, force, orgImg, skipHath)
              }
            }
              .onFailure {
                if (it is CancellationException) {
                  if (mReadReference > 0) {
                    logcat(WORKER_DEBUG_TAG) { "Download image $index cancelled" }
                    if (it.message != FORCE_RETRY) {
                      updatePageState(index, STATE_FAILED, "Cancelled")
                    }
                  }
                  throw it
                }
                updatePageState(index, STATE_FAILED, it.displayString())
              }
          }
        }
      }

      fun launch(index: Int, force: Boolean = false, orgImg: Boolean) {
        check(index in 0 until size)
        val state = pageStates[index]
        if (!force && state == STATE_FINISHED) return notifyPageReady(index)
        if (!isDownloadMode) {
          synchronized(jobs) { doLaunchDownloadJob(index, force, orgImg) }
        }
        launch {
          jobs[index]?.join()
          if (pageStates[index] == STATE_FINISHED) notifyPageReady(index)
        }
      }

      private suspend fun doInJob(index: Int, force: Boolean, orgImg: Boolean, skipHath: Boolean) {
        val selection =
          startReaderFetchPlanner(
            ReaderFetchState.initial(
              force,
              skipHath,
              Settings.downloadOriginImage.value || orgImg,
            )
          )
        val planner = selection.planner

        suspend fun getPToken(index: Int): String? {
          if (!isReady || index !in 0 until size) return null
          return spiderInfo.pTokenMap[index]
            ?: getPTokenFromMultiPageViewer(index)
            ?: getPTokenFromInternet(index)
            // Preview size may changed, so try to get pToken twice
            ?: getPTokenFromInternet(index)
        }

        var skipHathKey: String? = null
        var originImageUrl: String? = null
        var imageUrl: String? = null
        var resolvedOriginalTarget: String? = null
        var resolvedOriginalReferer: String? = null
        var error: String? = null
        var currentFailure: Throwable? = null
        var pToken: String? = null
        var previousPToken: String? = null
        var fetchState = selection.startPlan.state
        var bootstrapPlan = selection.startPlan

        fun failureMessage(source: ReaderFetchErrorSource?): String? =
          when (checkNotNull(source)) {
            ReaderFetchErrorSource.P_TOKEN_MESSAGE -> pTokenFailedMessage
            ReaderFetchErrorSource.RETAINED -> error
          }

        val acquiringTokensState: ReaderFetchState = pTokenLock.withLock {
          while (true) {
            fetchState = bootstrapPlan.state
            check(bootstrapPlan.actions.size == 1) {
              "Reader Fetch bootstrap plan must contain exactly one action"
            }
            val action = bootstrapPlan.actions.single()
            when (action.kind) {
              ReaderFetchActionKind.CHECK_CACHE -> {
                val event =
                  if (!force && index in spiderDen) {
                    ReaderFetchEvent.cacheHit()
                  } else {
                    ReaderFetchEvent.cacheMiss()
                  }
                bootstrapPlan = planner.next(fetchState, event)
              }
              ReaderFetchActionKind.ACQUIRE_TOKENS -> {
                val acquiredPToken = getPToken(index)
                if (acquiredPToken == null) {
                  bootstrapPlan = planner.next(fetchState, ReaderFetchEvent.tokensMissing())
                } else {
                  pToken = acquiredPToken
                  previousPToken = getPToken(index - 1)

                  // The lock for delay should be acquired before anything else to maintain FIFO
                  // order
                  delay(downloadDelay - lastRequestTime.elapsedNow())
                  lastRequestTime = TimeSource.Monotonic.markNow()
                  return@withLock fetchState
                }
              }
              ReaderFetchActionKind.SET_FINISHED -> {
                updatePageState(index, STATE_FINISHED)
                return
              }
              ReaderFetchActionKind.SET_FAILED -> {
                updatePageState(index, STATE_FAILED, failureMessage(action.error))
                return
              }
              else -> error("Reader Fetch emitted an invalid bootstrap action")
            }
          }
          error("Reader Fetch bootstrap loop terminated unexpectedly")
        }

        val activePToken = checkNotNull(pToken)
        val showKeyAvailable = showKeyLock.withLock { showKey != null }
        var plan =
          planner.next(
            acquiringTokensState,
            ReaderFetchEvent.tokensReady(showKeyAvailable),
          )
        while (true) {
          fetchState = plan.state
          var nextEvent: ReaderFetchEvent? = null
          var preappliedImageCleanup = false

          fun emitNext(event: ReaderFetchEvent) {
            check(nextEvent == null) {
              "Reader Fetch plan produced more than one next event"
            }
            nextEvent = event
          }

          for (action in plan.actions) {
            when (action.kind) {
              ReaderFetchActionKind.CHECK_CACHE,
              ReaderFetchActionKind.ACQUIRE_TOKENS -> {
                error("Reader Fetch bootstrap action escaped the token lock")
              }
              ReaderFetchActionKind.SET_DOWNLOADING -> {
                updatePageState(index, STATE_DOWNLOADING)
              }
              ReaderFetchActionKind.SET_FINISHED -> {
                updatePageState(index, STATE_FINISHED)
                return
              }
              ReaderFetchActionKind.SET_FAILED -> {
                updatePageState(index, STATE_FAILED, failureMessage(action.error))
                return
              }
              ReaderFetchActionKind.FETCH_PAGE -> {
                imageUrl = null
                val event =
                  when (checkNotNull(action.source)) {
                    ReaderFetchPageSource.HTML -> {
                      try {
                        val result = showKeyLock.withLock {
                          // Skipping H@H costs 50 points, only use it as last resort
                          val pageUrl =
                            EhUrl.getPageUrl(
                              galleryInfo.gid,
                              index,
                              activePToken,
                              skipHathKey,
                            )
                          EhEngine.getGalleryPage(
                              pageUrl,
                              galleryInfo.gid,
                              galleryInfo.token,
                            )
                            .also { result ->
                              check509(result.imageUrl)
                              imageUrl = result.imageUrl
                              skipHathKey = result.skipHathKey
                              originImageUrl = result.originImageUrl
                              showKey = result.showKey
                            }
                        }
                        ReaderFetchEvent.pageReady(
                          source = ReaderFetchPageSource.HTML,
                          originAvailable = originImageUrl != null,
                          skipKeyAvailable = skipHathKey != null,
                          showKeyAvailable = result.showKey != null,
                        )
                      } catch (failure: Throwable) {
                        if (failure is CancellationException) throw failure
                        currentFailure = failure
                        ReaderFetchEvent.outerFailureRaised(fatal = failure is FatalException)
                      }
                    }
                    ReaderFetchPageSource.API -> {
                      val localShowKey = showKeyLock.withLock { showKey }
                      runSuspendCatching {
                          EhEngine.getGalleryPageApi(
                            galleryInfo.gid,
                            index,
                            activePToken,
                            localShowKey,
                            previousPToken,
                          )
                        }
                        .fold(
                          onSuccess = { result ->
                            try {
                              check509(result.imageUrl)
                              imageUrl = result.imageUrl
                              skipHathKey = result.skipHathKey
                              originImageUrl = result.originImageUrl
                              ReaderFetchEvent.pageReady(
                                source = ReaderFetchPageSource.API,
                                originAvailable = originImageUrl != null,
                                skipKeyAvailable = skipHathKey != null,
                                showKeyAvailable = localShowKey != null,
                              )
                            } catch (failure: Throwable) {
                              if (failure is CancellationException) throw failure
                              currentFailure = failure
                              ReaderFetchEvent.outerFailureRaised(fatal = failure is FatalException)
                            }
                          },
                          onFailure = { failure ->
                            when (failure) {
                              is CancellationException -> throw failure
                              is FatalException -> {
                                currentFailure = failure
                                ReaderFetchEvent.outerFailureRaised(fatal = true)
                              }
                              else -> ReaderFetchEvent.apiRecoverableFailure()
                            }
                          },
                        )
                    }
                  }
                emitNext(event)
              }
              ReaderFetchActionKind.RESOLVE_ORIGINAL -> {
                val event =
                  try {
                    if (checkNotNull(action.appendSkipKey)) {
                      originImageUrl =
                        "${checkNotNull(originImageUrl)}?nl=${checkNotNull(skipHathKey)}"
                    }
                    val pageUrl =
                      EhUrl.getPageUrl(
                        galleryInfo.gid,
                        index,
                        activePToken,
                      )
                    resolvedOriginalTarget =
                      EhEngine.getOriginalImageUrl(
                        checkNotNull(originImageUrl),
                        pageUrl,
                      )
                    resolvedOriginalReferer = referer
                    ReaderFetchEvent.originalReady()
                  } catch (failure: Throwable) {
                    if (failure is CancellationException) throw failure
                    currentFailure = failure
                    ReaderFetchEvent.outerFailureRaised(fatal = failure is FatalException)
                  }
                emitNext(event)
              }
              ReaderFetchActionKind.DOWNLOAD_IMAGE -> {
                val (targetImageUrl, targetReferer) =
                  when (checkNotNull(action.target)) {
                    ReaderFetchImageTarget.ORIGINAL -> {
                      checkNotNull(resolvedOriginalTarget) to resolvedOriginalReferer
                    }
                    ReaderFetchImageTarget.RESIZED -> checkNotNull(imageUrl) to null
                  }
                try {
                  logcat(WORKER_DEBUG_TAG) { "Start download image $index" }
                  spiderDen.makeHttpCallAndSaveImage(
                    index,
                    targetImageUrl,
                    targetReferer,
                    this@SpiderQueen::notifyPageDownload.partially1(index),
                  )
                  logcat(WORKER_DEBUG_TAG) { "Download image $index succeed" }
                  emitNext(ReaderFetchEvent.imageSucceeded())
                } catch (failure: Throwable) {
                  currentFailure = failure
                  spiderDen.removeIntermediateFiles(index)
                  logcat(WORKER_DEBUG_TAG) { "Download image $index failed" }
                  val kind =
                    when (failure) {
                      is CancellationException -> ReaderFetchImageFailure.CANCELLATION
                      is FileNotFoundException -> ReaderFetchImageFailure.FILE_NOT_FOUND
                      else -> ReaderFetchImageFailure.RECOVERABLE
                    }
                  preappliedImageCleanup = true
                  emitNext(ReaderFetchEvent.imageFailed(kind))
                }
              }
              ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES -> {
                spiderDen.removeIntermediateFiles(index)
              }
              ReaderFetchActionKind.NOTIFY_FATAL -> {
                notifyFatal(
                  currentFailure as? FatalException
                    ?: error("Reader Fetch fatal notification has no fatal failure")
                )
              }
              ReaderFetchActionKind.CAPTURE_CURRENT_ERROR -> {
                error = checkNotNull(currentFailure).displayString()
                currentFailure = null
                if (fetchState.phase == ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE) {
                  emitNext(
                    ReaderFetchEvent.outerFailureClassified(invalidPage = error == "Invalid page.")
                  )
                }
              }
              ReaderFetchActionKind.INVALIDATE_TOKEN -> {
                pTokenLock.withLock {
                  spiderInfo.pTokenMap.remove(index)
                }
              }
              ReaderFetchActionKind.RETHROW_CANCELLATION -> {
                throw (currentFailure as? CancellationException
                  ?: error("Reader Fetch cancellation rethrow has no cancellation"))
              }
            }
          }

          check(
            fetchState.phase != ReaderFetchPhase.FINISHED &&
              fetchState.phase != ReaderFetchPhase.FAILED &&
              fetchState.phase != ReaderFetchPhase.CANCELLED
          ) {
            "Reader Fetch terminal plan did not terminate the job"
          }
          val event =
            nextEvent ?: error("Reader Fetch nonterminal plan did not produce a next event")
          val nextPlan = planner.next(fetchState, event)
          plan =
            if (preappliedImageCleanup) {
              nextPlan.consumePreappliedImageCleanup()
            } else {
              nextPlan
            }
        }
      }
    }
  private val pTokenFailedMessage = appCtx.getString(R.string.error_get_ptoken_error)
}

private val Url509Regex = Regex("https://(?:ehgt\\.org/|exhentai\\.org/im)g/509s?\\.gif")
private const val FORCE_RETRY = "Force retry"
private const val WORKER_DEBUG_TAG = "SpiderQueenWorker"

private fun check509(url: String) {
  if (Url509Regex.matches(url)) throw QuotaExceededException()
}
