/*
 * Copyright 2016 Hippo Seven
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
package com.hippo.ehviewer.download

import android.net.Uri
import android.util.SparseLongArray
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisallowComposableCalls
import androidx.compose.runtime.Stable
import androidx.compose.runtime.State
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.core.util.size
import arrow.fx.coroutines.parMapNotNull
import com.ehviewer.core.data.model.asEntity
import com.ehviewer.core.database.model.DownloadArtist
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.DownloadLabel
import com.ehviewer.core.files.delete
import com.ehviewer.core.files.find
import com.ehviewer.core.files.toOkioPath
import com.ehviewer.core.files.toUri
import com.ehviewer.core.model.BaseGalleryInfo
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.preferences.edit
import com.ehviewer.core.util.logcat
import com.ehviewer.core.util.mapNotNull
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.CoreBackend
import com.hippo.ehviewer.bridge.CoreBackendSelector
import com.hippo.ehviewer.bridge.download.DownloadProgressCommand
import com.hippo.ehviewer.bridge.download.DownloadProgressRequest
import com.hippo.ehviewer.bridge.download.DownloadQueueCommand
import com.hippo.ehviewer.bridge.download.DownloadQueuePlan
import com.hippo.ehviewer.bridge.download.DownloadSortRequest
import com.hippo.ehviewer.bridge.download.DownloadState
import com.hippo.ehviewer.bridge.download.buildDownloadQueueRecoveryEntries
import com.hippo.ehviewer.bridge.download.buildDownloadQueueSnapshot
import com.hippo.ehviewer.bridge.download.isExpectedDownloadProgressTask
import com.hippo.ehviewer.bridge.download.planDownloadProgress
import com.hippo.ehviewer.bridge.download.planLegacyDownloadProgress
import com.hippo.ehviewer.bridge.download.recoverDownloadQueue
import com.hippo.ehviewer.bridge.download.reduceDownloadQueue
import com.hippo.ehviewer.bridge.download.selectDownloadProgressPlan
import com.hippo.ehviewer.bridge.download.sortDownloadCatalog
import com.hippo.ehviewer.bridge.download.toAndroidDownloadState
import com.hippo.ehviewer.bridge.download.toDownloadProgressSnapshot
import com.hippo.ehviewer.client.exception.FatalException
import com.hippo.ehviewer.spider.COMIC_INFO_FILE
import com.hippo.ehviewer.spider.SpeedTracker
import com.hippo.ehviewer.spider.SpiderQueen
import com.hippo.ehviewer.spider.SpiderQueen.Companion.SPIDER_INFO_FILENAME
import com.hippo.ehviewer.spider.SpiderQueen.OnSpiderListener
import com.hippo.ehviewer.spider.downloadDirname
import com.hippo.ehviewer.spider.readComicInfo
import com.hippo.ehviewer.spider.readCompatFromPath
import com.hippo.ehviewer.spider.toSimpleTags
import com.hippo.ehviewer.util.AppConfig
import com.hippo.ehviewer.util.runAssertingNotMainThread
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.transform
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import logcat.LogPriority
import okio.Path
import okio.Path.Companion.toOkioPath
import okio.Path.Companion.toPath

object DownloadManager : OnSpiderListener, CoroutineScope {
  override val coroutineContext = Dispatchers.IO + Job()

  // All download info list
  private val allInfoList = runAssertingNotMainThread { EhDB.getAllDownloadInfo() as MutableList }

  val downloadInfoList: List<DownloadInfo>
    get() = allInfoList

  // All download info map
  private val mAllInfoMap = allInfoList.associateBy { it.gid } as MutableMap<Long, DownloadInfo>

  // All labels without default label
  // Create the SnapshotStateList first in case the database query is time-consuming
  val labelList =
    mutableStateListOf<DownloadLabel>().apply {
      addAll(runAssertingNotMainThread { EhDB.getAllDownloadLabelList() })
    }

  // Store download info wait to start
  private val mWaitList = ArrayDeque<DownloadInfo>()
  private var mDownloadListener: DownloadListener? = null
  private var mCurrentTask: DownloadInfo? = null
  private var mCurrentSpider: SpiderQueen? = null
  private var mCurrentBackend: CoreBackend? = null

  private val mutableNotifyFlow = MutableSharedFlow<DownloadInfo>(extraBufferCapacity = 1)
  val notifyFlow = mutableNotifyFlow.asSharedFlow()

  private val mutex = Mutex()

  private val sortMutex = Mutex()
  private val sortMode
    get() = SortMode.from(Settings.downloadSortMode.value)

  var isInitialized by mutableStateOf(false)
    private set

  init {
    launch {
      val backend = CoreBackendSelector.snapshot()
      if (backend == CoreBackend.RUST) {
        recoverRustDownloadQueue()
      }
      val mode = sortMode
      if (mode != SortMode.Default) {
        sortMutex.withLock { sortDownloadsLocked(mode, backend) }
      }
      isInitialized = true
    }
  }

  fun containLabel(label: String?): Boolean {
    if (label == null) {
      return false
    }
    return labelList.any { it.label == label }
  }

  fun containDownloadInfo(gid: Long) = mAllInfoMap.containsKey(gid)

  fun getDownloadInfo(gid: Long) = mAllInfoMap[gid]

  fun getDownloadState(gid: Long): Int {
    val info = mAllInfoMap[gid]
    return info?.state ?: DownloadInfo.STATE_INVALID
  }

  fun setDownloadListener(listener: DownloadListener?) {
    mDownloadListener = listener
  }

  private suspend fun ensureDownload(backend: CoreBackend) {
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.activateNext())
      return
    }
    mutex.withLock {
      if (mCurrentTask != null) {
        // Only one download
        return
      }

      // Get download from wait list
      if (!mWaitList.isEmpty()) {
        val info = mWaitList.removeFirst()
        mCurrentTask = info
        mCurrentBackend = CoreBackend.LEGACY
        with(info) {
          archiveFile?.let {
            return onFinish(pages, pages, pages, failed = false)
          }
          state = DownloadInfo.STATE_DOWNLOAD
          speed = -1
          remaining = -1
          total = -1
          finished = 0
          downloaded = 0
          legacy = -1
        }
        val spider = SpiderQueen.obtainSpiderQueen(info, SpiderQueen.MODE_DOWNLOAD)
        mCurrentSpider = spider
        spider.addOnSpiderListener(this)
        // Update in DB
        EhDB.putDownloadInfo(info)
        // Start speed count
        mSpeedReminder.start()
        // Notify start downloading
        mDownloadListener?.onStart(info)
        // Notify state update
        mutableNotifyFlow.emit(info)
      }
    }
  }

  suspend fun startDownload(galleryInfo: BaseGalleryInfo, label: String?) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      startRustDownload(galleryInfo, label)
      return
    }
    if (mCurrentTask?.gid == galleryInfo.gid) {
      // It is current task
      return
    }

    // Check in download list
    var info = mAllInfoMap[galleryInfo.gid]
    if (info != null) { // Get it in download list
      if (info.state != DownloadInfo.STATE_WAIT) {
        // Set state DownloadInfo.STATE_WAIT
        info.state = DownloadInfo.STATE_WAIT
        // Add to wait list
        mWaitList.add(info)
        // Update in DB
        EhDB.putDownloadInfo(info)
        // Notify state update
        mutableNotifyFlow.emit(info)
        // Make sure download is running
        ensureDownload(backend)
      }
    } else {
      // It is new download info
      info = DownloadInfo(galleryInfo.asEntity(), galleryInfo.downloadDirname())
      info.label = label
      info.state = DownloadInfo.STATE_WAIT
      // Add to all download list and map
      sortMutex.withLock {
        insertDownloadInfoLocked(info, sortMode, backend)
      }
      mAllInfoMap[galleryInfo.gid] = info

      // Add to wait list
      mWaitList.add(info)

      // Save to
      EhDB.putDownloadInfo(info)

      // Notify
      mutableNotifyFlow.emit(info)
      // Make sure download is running
      ensureDownload(backend)

      // Add it to history
      EhDB.putHistoryInfo(info)
    }
  }

  suspend fun startRangeDownload(gidList: LongArray) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.startRange(gidList.toList()))
      return
    }
    val updateList =
      gidList
        .mapNotNull { mAllInfoMap[it] }
        .filter {
          when (it.state) {
            DownloadInfo.STATE_NONE,
            DownloadInfo.STATE_FAILED,
            DownloadInfo.STATE_FINISH -> true
            else -> false
          }
        }
    if (updateList.isNotEmpty()) {
      updateList.onEach {
        it.state = DownloadInfo.STATE_WAIT
        EhDB.putDownloadInfo(it)
        mutableNotifyFlow.emit(it)
      }
      mWaitList.addAll(updateList)
      ensureDownload(backend)
    }
  }

  @Stable
  @Composable
  fun collectDownloadState(gid: Long): State<Int> = remember {
    notifyFlow.transform { if (it.gid == gid) emit(getDownloadState(gid)) }
  }
    .collectAsState(getDownloadState(gid))

  @Stable
  @Composable
  fun collectContainDownloadInfo(gid: Long): State<Boolean> = remember {
    notifyFlow.transform { if (it.gid == gid) emit(containDownloadInfo(gid)) }
  }
    .collectAsState(containDownloadInfo(gid))

  @Stable
  @Composable
  inline fun <T> updatedDownloadInfo(
    info: DownloadInfo,
    crossinline transform: @DisallowComposableCalls DownloadInfo.() -> T,
  ): T = remember {
    notifyFlow.transform { if (it.gid == info.gid) emit(transform(it)) }
  }
    .collectAsState(transform(info))
    .value

  suspend fun startAllDownload() {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.startAll())
      return
    }
    val updateList = sortMutex.withLock {
      allInfoList.filter {
        when (it.state) {
          DownloadInfo.STATE_NONE,
          DownloadInfo.STATE_FAILED -> true
          else -> false
        }
      }
    }
    if (updateList.isNotEmpty()) {
      updateList.onEach {
        it.state = DownloadInfo.STATE_WAIT
        EhDB.putDownloadInfo(it)
        mutableNotifyFlow.emit(it)
      }
      mWaitList.addAll(updateList)
      ensureDownload(backend)
    }
  }

  suspend fun addDownload(downloadInfoList: List<DownloadInfo>) {
    val backend = CoreBackendSelector.snapshot()
    sortMutex.withLock {
      val additions =
        downloadInfoList
          .filterNot { containDownloadInfo(it.gid) }
          .distinctBy { it.gid }
          .onEach { info ->
            if (
              DownloadInfo.STATE_WAIT == info.state || DownloadInfo.STATE_DOWNLOAD == info.state
            ) {
              info.state = DownloadInfo.STATE_NONE
            }
          }
      if (additions.isEmpty()) return@withLock
      val ordered = orderDownloadInfos(allInfoList + additions, sortMode, backend)
      additions.forEach { info ->
        mAllInfoMap[info.gid] = info
        EhDB.putDownloadInfo(info)
        EhDB.putDownloadArtist(info.gid, info.artistInfoList)
      }
      allInfoList.clear()
      allInfoList.addAll(ordered)
    }
  }

  suspend fun addDownloadLabel(downloadLabelList: List<DownloadLabel>) {
    val offset = downloadLabelList.size
    downloadLabelList.forEachIndexed { index, label ->
      if (!containLabel(label.label)) {
        label.position = index + offset
        labelList.add(EhDB.addDownloadLabel(label))
      }
    }
  }

  suspend fun restoreDownload(galleryInfo: BaseGalleryInfo, dirname: String) {
    val backend = CoreBackendSelector.snapshot()
    val info = DownloadInfo(galleryInfo.asEntity(), dirname)
    info.state = DownloadInfo.STATE_NONE

    // Add to all download list and map
    sortMutex.withLock {
      insertDownloadInfoLocked(info, sortMode, backend)
    }
    mAllInfoMap[galleryInfo.gid] = info

    // Save to
    EhDB.putDownloadInfo(info)

    // Notify
    mutableNotifyFlow.emit(info)
  }

  /**
   * Reconciles a verified recovery only after its Room promotion transaction committed.
   *
   * This method never writes Room and never turns an unresolved source into a queue item.
   */
  suspend fun reconcileVerifiedRecovery(gid: Long) {
    val restored = requireNotNull(EhDB.getDownloadInfo(gid)) {
      "Promoted recovery is missing from Room"
    }
    require(restored.state == DownloadInfo.STATE_FINISH) {
      "Promoted recovery is not marked finished"
    }
    val backend = CoreBackendSelector.snapshot()
    sortMutex.withLock {
      mutex.withLock {
        require(mCurrentTask?.gid != gid && mWaitList.none { it.gid == gid }) {
          "An active download cannot be reconciled as recovered"
        }
        mAllInfoMap.remove(gid)?.let(allInfoList::remove)
        insertDownloadInfoLocked(restored, sortMode, backend)
        mAllInfoMap[gid] = restored
      }
    }
    mutableNotifyFlow.emit(restored)
  }

  suspend fun stopDownload(gid: Long) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.stopOne(gid))
      return
    }
    val info = stopDownloadInternal(gid)
    if (info != null) {
      // Update listener
      mutableNotifyFlow.emit(info)
      // Ensure download
      ensureDownload(backend)
    }
  }

  suspend fun stopRangeDownload(gidList: LongArray) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.stopRange(gidList.toList()))
      return
    }
    stopRangeDownloadInternal(gidList)

    // Update listener
    gidList
      .mapNotNull { mAllInfoMap[it] }
      .forEach {
        mutableNotifyFlow.emit(it)
      }

    // Ensure download
    ensureDownload(backend)
  }

  suspend fun stopAllDownload() = stopAllDownload(CoreBackendSelector.snapshot())

  private suspend fun stopAllDownload(backend: CoreBackend) {
    if (backend == CoreBackend.RUST) {
      runRustDownloadQueueCommand(DownloadQueueCommand.stopAll())
      return
    }
    mutex.withLock {
      // Stop all in wait list
      for (info in mWaitList) {
        info.state = DownloadInfo.STATE_NONE
        // Update in DB
        EhDB.putDownloadInfo(info)

        // Notify
        mutableNotifyFlow.emit(info)
      }
      mWaitList.clear()

      // Stop current
      val info = stopCurrentDownloadInternal()
      info?.let { mutableNotifyFlow.emit(it) }
    }
  }

  suspend fun deleteDownload(gid: Long, deleteFiles: Boolean = false) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      val info = deleteRustDownload(gid)
      if (deleteFiles && info != null) {
        deleteDownloadFiles(info)
      }
      return
    }
    stopDownloadInternal(gid)
    val info = mAllInfoMap[gid]
    if (info != null) {
      // Remove from DB
      EhDB.removeDownloadInfo(info)

      // Remove all list and map
      sortMutex.withLock {
        allInfoList.remove(info)
      }
      mAllInfoMap.remove(info.gid)

      // Update listener
      mutableNotifyFlow.emit(info)

      // Ensure download
      ensureDownload(backend)

      if (deleteFiles) {
        deleteDownloadFiles(info)
      }
    }
  }

  suspend fun deleteRangeDownload(gidList: LongArray) {
    val backend = CoreBackendSelector.snapshot()
    if (backend == CoreBackend.RUST) {
      deleteRustDownloads(gidList)
      return
    }
    stopRangeDownloadInternal(gidList)
    val list = gidList.mapNotNull { gid ->
      mAllInfoMap.remove(gid)
    }
    EhDB.removeDownloadInfo(list)
    sortMutex.withLock {
      allInfoList.removeAll(list.toSet())
    }

    // Update listener
    list.forEach { mutableNotifyFlow.emit(it) }

    // Ensure download
    ensureDownload(backend)
  }

  suspend fun sortDownloads(mode: SortMode) {
    val backend = CoreBackendSelector.snapshot()
    sortMutex.withLock { sortDownloadsLocked(mode, backend) }
  }

  suspend fun resetAllReadingProgress() = runCatching {
    EhDB.clearProgressInfo()
  }
    .onFailure { logcat(it) }

  /** Creates a missing Android row before asking Rust to start exactly one task. */
  private suspend fun startRustDownload(galleryInfo: BaseGalleryInfo, label: String?) {
    sortMutex.withLock {
      mutex.withLock {
        recoverRustDownloadQueueIfNeededLocked()
        var info = mAllInfoMap[galleryInfo.gid]
        val inserted = info == null
        if (info == null) {
          info = DownloadInfo(galleryInfo.asEntity(), galleryInfo.downloadDirname())
          info.label = label
          info.state = DownloadInfo.STATE_NONE
          insertDownloadInfoLocked(info, sortMode, CoreBackend.RUST)
          mAllInfoMap[info.gid] = info
        }

        val plan =
          try {
            createRustDownloadQueuePlanLocked(DownloadQueueCommand.startOne(info.gid))
          } catch (error: Throwable) {
            if (inserted) {
              allInfoList.remove(info)
              mAllInfoMap.remove(info.gid)
            }
            throw error
          }
        applyRustDownloadQueuePlanLocked(plan)

        if (inserted) {
          EhDB.putHistoryInfo(info)
        }
      }
    }
  }

  /** Stops and removes one item without allowing another queue command to interleave. */
  private suspend fun deleteRustDownload(galleryId: Long): DownloadInfo? = sortMutex.withLock {
    mutex.withLock {
      recoverRustDownloadQueueIfNeededLocked()
      reduceRustDownloadQueueLocked(DownloadQueueCommand.stopOne(galleryId))
      mAllInfoMap.remove(galleryId)?.also { info ->
        EhDB.removeDownloadInfo(info)
        allInfoList.remove(info)
        mutableNotifyFlow.emit(info)
      }
    }
  }

  /** Stops and removes selected items atomically with respect to Rust queue commands. */
  private suspend fun deleteRustDownloads(galleryIds: LongArray) {
    sortMutex.withLock {
      mutex.withLock {
        recoverRustDownloadQueueIfNeededLocked()
        reduceRustDownloadQueueLocked(DownloadQueueCommand.stopRange(galleryIds.toList()))
        val infos = galleryIds.mapNotNull(mAllInfoMap::remove)
        EhDB.removeDownloadInfo(infos)
        allInfoList.removeAll(infos.toSet())
        infos.forEach { mutableNotifyFlow.emit(it) }
      }
    }
  }

  /** Removes Android-owned files and the persisted directory mapping after queue deletion. */
  private suspend fun deleteDownloadFiles(info: DownloadInfo) {
    info.downloadDir?.delete()
    info.tempDownloadDir?.delete()
    EhDB.removeDownloadDirname(info.gid)
  }

  /** Serializes one pure Rust queue transition with Android's list and worker state. */
  private suspend fun runRustDownloadQueueCommand(
    command: DownloadQueueCommand,
    completion: DownloadCompletion? = null,
  ) {
    sortMutex.withLock {
      mutex.withLock {
        if (
          completion != null &&
            (mCurrentTask !== completion.expectedTask || mCurrentBackend != completion.backend)
        ) {
          logcat(TAG, LogPriority.WARN) { "Ignoring stale download callback" }
        } else {
          recoverRustDownloadQueueIfNeededLocked()
          reduceRustDownloadQueueLocked(command, completion)
        }
      }
    }
  }

  private fun insertDownloadInfoLocked(
    info: DownloadInfo,
    mode: SortMode,
    backend: CoreBackend,
  ) {
    val ordered = orderDownloadInfos(allInfoList + info, mode, backend)
    allInfoList.clear()
    allInfoList.addAll(ordered)
  }

  private fun sortDownloadsLocked(mode: SortMode, backend: CoreBackend) {
    val ordered = orderDownloadInfos(allInfoList, mode, backend)
    allInfoList.clear()
    allInfoList.addAll(ordered)
  }

  private fun orderDownloadInfos(
    source: List<DownloadInfo>,
    mode: SortMode,
    backend: CoreBackend,
  ): List<DownloadInfo> =
    when (backend) {
      CoreBackend.LEGACY -> source.sortedWith(mode.comparator())
      CoreBackend.RUST -> {
        val request = DownloadSortRequest.from(source, mode, Settings.showJpnTitle.value)
        val byGalleryId = source.associateBy { it.gid }
        sortDownloadCatalog(request).galleryIds.map { galleryId ->
          checkNotNull(byGalleryId[galleryId]) { "Rust sorted an unknown download" }
        }
      }
    }

  /** Recovers volatile persisted states without activating a worker. */
  private suspend fun recoverRustDownloadQueue() {
    sortMutex.withLock {
      mutex.withLock {
        recoverRustDownloadQueueIfNeededLocked()
      }
    }
  }

  /** Resets only persisted volatile rows that are absent from the live Android topology. */
  private suspend fun recoverRustDownloadQueueIfNeededLocked() {
    val orphanEntries = buildDownloadQueueRecoveryEntries(allInfoList, mWaitList, mCurrentTask)
    if (orphanEntries.isEmpty()) {
      return
    }
    val recovery = recoverDownloadQueue(orphanEntries)
    recovery.changes.forEach { change ->
      val info = checkNotNull(mAllInfoMap[change.galleryId])
      info.state = change.current.toAndroidDownloadState()
      EhDB.putDownloadInfo(info)
      mutableNotifyFlow.emit(info)
    }
  }

  /** Reduces and applies one command while the caller holds both queue locks. */
  private suspend fun reduceRustDownloadQueueLocked(
    command: DownloadQueueCommand,
    completion: DownloadCompletion? = null,
  ) {
    applyRustDownloadQueuePlanLocked(createRustDownloadQueuePlanLocked(command), completion)
  }

  /** Builds the authoritative snapshot and obtains one fully validated Rust decision. */
  private fun createRustDownloadQueuePlanLocked(command: DownloadQueueCommand): DownloadQueuePlan {
    val snapshot = buildDownloadQueueSnapshot(allInfoList, mWaitList, mCurrentTask)
    return reduceDownloadQueue(snapshot, command)
  }

  /** Applies a validated Rust plan while keeping all Android-only effects on this side. */
  private suspend fun applyRustDownloadQueuePlanLocked(
    plan: DownloadQueuePlan,
    completion: DownloadCompletion? = null,
  ) {
    require(completion == null || plan.cancelGalleryId == null) {
      "A completed Download Queue task cannot also be cancelled"
    }
    require(completion == null || completion.backend == CoreBackend.RUST) {
      "A Rust Download Queue completion must belong to a Rust task"
    }
    val cancelledInfo =
      plan.cancelGalleryId?.let { galleryId ->
        checkNotNull(detachCurrentWorkerLocked(galleryId))
      }
    val completedInfo = completion?.let { result ->
      val progressPlan = mCurrentTask?.let { info ->
        check(mCurrentBackend == CoreBackend.RUST) { "Active download backend changed" }
        val command =
          DownloadProgressCommand.finish(
            result.finished,
            result.downloaded,
            result.total,
            failed = result.failed,
          )
        planDownloadProgress(DownloadProgressRequest(1, info.toDownloadProgressSnapshot(), command))
      }
      if (progressPlan != null) {
        val expectedState = plan.snapshot.entries.single { it.galleryId == result.galleryId }.state
        require(progressPlan.snapshot.state == expectedState) {
          "Download progress state does not match queue finish state"
        }
      }
      mSpeedReminder.onFinish()
      checkNotNull(detachCurrentWorkerLocked(result.galleryId)).also { info ->
        if (progressPlan != null) {
          val snapshot = progressPlan.snapshot
          info.finished = snapshot.finished
          info.downloaded = snapshot.downloaded
          info.total = snapshot.total
          info.legacy = snapshot.legacy
          info.speed = snapshot.speedBytesPerSecond
          info.remaining = snapshot.remainingMilliseconds
          info.state = snapshot.state.toAndroidDownloadState()
        } else {
          info.finished = result.finished
          info.downloaded = result.downloaded
          info.total = result.total
          info.legacy = result.total - result.finished
        }
      }
    }

    mWaitList.clear()
    plan.snapshot.waiting.forEach { galleryId ->
      mWaitList.add(checkNotNull(mAllInfoMap[galleryId]))
    }
    val finalCurrentInfo = plan.snapshot.current?.let { checkNotNull(mAllInfoMap[it]) }
    mCurrentTask = finalCurrentInfo.takeIf { plan.startGalleryId == null }
    mCurrentBackend = mCurrentTask?.let { CoreBackend.RUST }

    val changedInfos =
      plan.changes.map { change ->
        val info = checkNotNull(mAllInfoMap[change.galleryId])
        if (change.current == DownloadState.DOWNLOADING) {
          applyDownloadProgress(info, DownloadProgressCommand.reset(), CoreBackend.RUST)
        }
        info.state = change.current.toAndroidDownloadState()
        EhDB.putDownloadInfo(info)
        info
      }

    val startGalleryId = plan.startGalleryId
    changedInfos
      .filter { it.gid != startGalleryId }
      .forEach { info ->
        when (info.gid) {
          cancelledInfo?.gid -> mDownloadListener?.onCancel(info)
          completedInfo?.gid -> mDownloadListener?.onFinish(info)
        }
        mutableNotifyFlow.emit(info)
      }

    startGalleryId?.let { galleryId ->
      val info = checkNotNull(mAllInfoMap[galleryId])
      check(finalCurrentInfo === info) {
        "Download Queue start effect does not match final current"
      }
      mCurrentTask = info
      mCurrentBackend = CoreBackend.RUST
      if (info.archiveFile != null) {
        mutableNotifyFlow.emit(info)
        onFinish(info.pages, info.pages, info.pages, failed = false)
        return@let
      }
      val spider = SpiderQueen.obtainSpiderQueen(info, SpiderQueen.MODE_DOWNLOAD)
      mCurrentSpider = spider
      spider.addOnSpiderListener(this)
      mSpeedReminder.start()
      mDownloadListener?.onStart(info)
      mutableNotifyFlow.emit(info)
    }
  }

  /** Plans and applies one immutable runtime-progress transition. */
  private fun applyDownloadProgress(
    info: DownloadInfo,
    command: DownloadProgressCommand,
    backend: CoreBackend,
  ) {
    val current = info.toDownloadProgressSnapshot()
    val selected =
      selectDownloadProgressPlan(
        backend = backend,
        legacyPlanner = { planLegacyDownloadProgress(current, command) },
        rustPlanner = {
          planDownloadProgress(DownloadProgressRequest(1, current, command))
        },
      )
    val snapshot = selected.snapshot
    info.speed = snapshot.speedBytesPerSecond
    info.remaining = snapshot.remainingMilliseconds
    info.finished = snapshot.finished
    info.downloaded = snapshot.downloaded
    info.total = snapshot.total
    info.legacy = snapshot.legacy
    info.state = snapshot.state.toAndroidDownloadState()
  }

  /** Applies a callback only while both the captured task and its backend are still current. */
  private suspend fun withExpectedDownloadProgressTask(
    expected: DownloadInfo?,
    expectedBackend: CoreBackend?,
    action: suspend (DownloadInfo, CoreBackend) -> Unit,
  ) {
    sortMutex.withLock sortLock@{
      mutex.withLock mainLock@{
        if (
          !isExpectedDownloadProgressTask(mCurrentTask, expected) ||
            expectedBackend == null ||
            mCurrentBackend != expectedBackend
        ) {
          logcat(TAG, LogPriority.WARN) { "Ignoring stale download callback" }
          return@mainLock
        }
        action(checkNotNull(expected), expectedBackend)
      }
    }
  }

  /** Releases the active Android worker without deciding or persisting its next state. */
  private suspend fun detachCurrentWorkerLocked(expectedGalleryId: Long? = null): DownloadInfo? {
    val info = mCurrentTask
    if (expectedGalleryId != null) {
      check(info?.gid == expectedGalleryId) {
        "Active download does not match expected task"
      }
    }
    mCurrentSpider?.let { spider ->
      spider.removeOnSpiderListener(this@DownloadManager)
      SpiderQueen.releaseSpiderQueen(spider, SpiderQueen.MODE_DOWNLOAD)
    }
    mCurrentTask = null
    mCurrentSpider = null
    mCurrentBackend = null
    mSpeedReminder.stop()
    return info
  }

  // Update in DB
  // Update listener
  // No ensureDownload
  private suspend fun stopDownloadInternal(gid: Long): DownloadInfo? {
    // Check current task
    if (mCurrentTask?.gid == gid) {
      // Stop current
      return stopCurrentDownloadInternal()
    }
    val iterator = mWaitList.iterator()
    while (iterator.hasNext()) {
      val info = iterator.next()
      if (info.gid == gid) {
        // Remove from wait list
        iterator.remove()
        // Update state
        info.state = DownloadInfo.STATE_NONE
        // Update in DB
        EhDB.putDownloadInfo(info)
        return info
      }
    }
    return null
  }

  // Update in DB
  // Update mDownloadListener
  private suspend fun stopCurrentDownloadInternal(): DownloadInfo? {
    val info = detachCurrentWorkerLocked()
    if (info == null) {
      return null
    }

    // Update state
    info.state = DownloadInfo.STATE_NONE
    // Update in DB
    EhDB.putDownloadInfo(info)
    // Listener
    mDownloadListener?.onCancel(info)
    return info
  }

  // Update in DB
  // Update mDownloadListener
  private suspend fun stopRangeDownloadInternal(gidList: LongArray) {
    // Two way
    if (gidList.size < mWaitList.size) {
      for (element in gidList) {
        stopDownloadInternal(element)
      }
    } else {
      // Check current task
      if (mCurrentTask != null && gidList.contains(mCurrentTask!!.gid)) {
        // Stop current
        stopCurrentDownloadInternal()
      }

      // Check all in wait list
      val iterator = mWaitList.iterator()
      while (iterator.hasNext()) {
        val info = iterator.next()
        if (gidList.contains(info.gid)) {
          // Remove from wait list
          iterator.remove()
          // Update state
          info.state = DownloadInfo.STATE_NONE
          // Update in DB
          EhDB.putDownloadInfo(info)
        }
      }
    }
  }

  /** @param label Not allow new label */
  suspend fun changeLabel(list: Collection<DownloadInfo>, label: String?) {
    if (null != label && !containLabel(label)) {
      logcat(TAG, LogPriority.ERROR) { "Not exits label: $label" }
      return
    }
    list.forEach {
      it.label = label
    }
    EhDB.updateDownloadInfo(list)
  }

  suspend fun addLabel(label: String?) {
    if (label == null || containLabel(label)) {
      return
    }
    labelList.add(EhDB.addDownloadLabel(DownloadLabel(label, labelList.size)))
  }

  suspend fun renameLabel(from: String, to: String) {
    val index = labelList.indexOfFirst { it.label == from }
    if (index != -1) {
      val exist = labelList.removeAt(index)
      val new = exist.copy(label = to)
      labelList.add(index, new)
      EhDB.updateDownloadLabel(new)
      sortMutex.withLock {
        allInfoList.forEach {
          if (it.label == from) {
            it.label = to
          }
        }
      }
    }
  }

  suspend fun deleteLabel(label: String) {
    with(labelList) {
      val index = indexOfFirst { it.label == label }
      val item = get(index)
      EhDB.removeDownloadLabel(item)
      subList(index + 1, size).forEach {
        it.position--
      }
      removeAt(index)
    }
    sortMutex.withLock {
      allInfoList.forEach {
        if (it.label == label) {
          it.label = null
        }
      }
    }
  }

  suspend fun readMetadataFromLocal() {
    val list =
      sortMutex
        .withLock {
          allInfoList.mapNotNull {
            val updateGallery = it.pages == 0 || it.simpleTags == null
            val updateArtist = it.artistInfoList.isEmpty()
            if (updateGallery || updateArtist) {
              Triple(it, updateGallery, updateArtist)
            } else {
              null
            }
          }
        }
        .parMapNotNull(concurrency = 5) { (info, updateGallery, updateArtist) ->
          info.downloadDir?.run {
            val comicInfo = find(COMIC_INFO_FILE)?.let { readComicInfo(it) }
            if (comicInfo != null) {
              val galleryInfo =
                if (updateGallery) {
                  info.pages = comicInfo.pageCount
                  info.simpleTags = comicInfo.toSimpleTags()
                  info.galleryInfo
                } else {
                  null
                }

              val artistList =
                if (updateArtist && comicInfo.penciller != null) {
                  info.artistInfoList = DownloadArtist.from(info.gid, comicInfo.penciller)
                  info.gid to info.artistInfoList
                } else {
                  null
                }

              if (galleryInfo != null || artistList != null) {
                galleryInfo to artistList
              } else {
                null
              }
            } else if (info.pages == 0) {
              val galleryInfo =
                find(SPIDER_INFO_FILENAME)?.let {
                  readCompatFromPath(it)?.run {
                    info.pages = pages
                    info.galleryInfo
                  }
                }

              galleryInfo?.let { it to null }
            } else {
              null
            }
          }
        }

    val galleryInfoList = buildList {
      list.forEach { (info, artists) ->
        info?.let {
          add(it)
        }
        artists?.let { (gid, updateList) ->
          EhDB.putDownloadArtist(gid, updateList)
        }
      }
    }
    if (galleryInfoList.isNotEmpty()) EhDB.updateGalleryInfo(galleryInfoList)
  }

  val isIdle: Boolean
    get() = mCurrentTask == null && mWaitList.isEmpty()

  override fun onGetPages(pages: Int) {
    val expected = mCurrentTask
    val backend = mCurrentBackend
    launch {
      withExpectedDownloadProgressTask(expected, backend) { info, selectedBackend ->
        applyDownloadProgress(info, DownloadProgressCommand.setPages(pages), selectedBackend)
        mutableNotifyFlow.emit(info)
      }
    }
  }

  override fun onFatal(error: FatalException) {
    val backend = mCurrentBackend ?: CoreBackendSelector.snapshot()
    launch {
      mDownloadListener?.onFatal(error)
      stopAllDownload(backend)
    }
  }

  override fun onPageDownload(
    index: Int,
    contentLength: Long,
    receivedSize: Long,
    bytesRead: Int,
  ) {
    val expected = mCurrentTask
    val backend = mCurrentBackend
    launch {
      withExpectedDownloadProgressTask(expected, backend) { _, _ ->
        mSpeedReminder.onDownload(index, contentLength, receivedSize, bytesRead)
      }
    }
  }

  override fun onPageSuccess(index: Int, finished: Int, downloaded: Int, total: Int) {
    val expected = mCurrentTask
    val backend = mCurrentBackend
    launch {
      withExpectedDownloadProgressTask(expected, backend) { info, selectedBackend ->
        mSpeedReminder.onDone(index)
        applyDownloadProgress(
          info,
          DownloadProgressCommand.setPageResult(finished, downloaded, total),
          selectedBackend,
        )
        mDownloadListener?.onDownload(info)
        mutableNotifyFlow.emit(info)
      }
    }
  }

  override fun onPageFailure(
    index: Int,
    error: String?,
    finished: Int,
    downloaded: Int,
    total: Int,
  ) {
    val expected = mCurrentTask
    val backend = mCurrentBackend
    launch {
      withExpectedDownloadProgressTask(expected, backend) { info, selectedBackend ->
        mSpeedReminder.onDone(index)
        applyDownloadProgress(
          info,
          DownloadProgressCommand.setPageResult(finished, downloaded, total),
          selectedBackend,
        )
        mutableNotifyFlow.emit(info)
      }
    }
  }

  override fun onFinish(finished: Int, downloaded: Int, total: Int, failed: Boolean) {
    val expected = mCurrentTask
    val backend = mCurrentBackend
    launch {
      when (backend) {
        CoreBackend.RUST ->
          if (expected == null) {
            logcat(TAG, LogPriority.ERROR) { "Current task is null, but it should not be" }
          } else {
            runRustDownloadQueueCommand(
              DownloadQueueCommand.finish(expected.gid, failed),
              DownloadCompletion(expected, backend, finished, downloaded, total, failed),
            )
          }
        CoreBackend.LEGACY ->
          if (expected == null) {
            logcat(TAG, LogPriority.ERROR) { "Current task is null, but it should not be" }
          } else if (
            finishLegacyDownload(
              DownloadCompletion(expected, backend, finished, downloaded, total, failed)
            )
          ) {
            ensureDownload(backend)
          }
        null -> logcat(TAG, LogPriority.ERROR) { "Current task backend is null" }
      }
    }
  }

  private suspend fun finishLegacyDownload(completion: DownloadCompletion): Boolean =
    sortMutex.withLock {
      mutex.withLock {
        if (mCurrentTask !== completion.expectedTask || mCurrentBackend != completion.backend) {
          logcat(TAG, LogPriority.WARN) { "Ignoring stale download callback" }
          false
        } else {
          mSpeedReminder.onFinish()
          val info = checkNotNull(detachCurrentWorkerLocked(completion.galleryId))
          applyDownloadProgress(
            info,
            DownloadProgressCommand.finish(
              completion.finished,
              completion.downloaded,
              completion.total,
              completion.failed,
            ),
            completion.backend,
          )
          EhDB.putDownloadInfo(info)
          mDownloadListener?.onFinish(info)
          mutableNotifyFlow.emit(info)
          true
        }
      }
    }

  private data class DownloadCompletion(
    val expectedTask: DownloadInfo,
    val backend: CoreBackend,
    val finished: Int,
    val downloaded: Int,
    val total: Int,
    val failed: Boolean,
  ) {
    val galleryId: Long
      get() = expectedTask.gid
  }

  interface DownloadListener {
    /** Fatal error */
    fun onFatal(error: FatalException)

    /** Start download */
    fun onStart(info: DownloadInfo)

    /** Update download speed */
    fun onDownload(info: DownloadInfo)

    /** Download done */
    fun onFinish(info: DownloadInfo)

    /** Download done */
    fun onCancel(info: DownloadInfo)
  }

  private val mSpeedReminder =
    object {
      private val contentLengthMap = SparseLongArray()
      private val receivedSizeMap = SparseLongArray()
      private val tracker = SpeedTracker()
      private val mutex = Mutex()
      private var currentJob: Job? = null

      fun start() {
        if (currentJob == null) {
          currentJob = launch {
            tracker.speedFlow().collect(::updateSpeed)
          }
        }
      }

      suspend fun stop() {
        tracker.reset()
        mutex.withLock {
          contentLengthMap.clear()
          receivedSizeMap.clear()
        }
        currentJob?.cancel()
        currentJob = null
      }

      suspend fun onDownload(index: Int, contentLength: Long, receivedSize: Long, bytesRead: Int) {
        mutex.withLock {
          contentLengthMap.put(index, contentLength)
          receivedSizeMap.put(index, receivedSize)
        }
        tracker.track(bytesRead)
      }

      suspend fun onDone(index: Int) {
        mutex.withLock {
          contentLengthMap.delete(index)
          receivedSizeMap.delete(index)
        }
      }

      suspend fun onFinish() {
        mutex.withLock {
          contentLengthMap.clear()
          receivedSizeMap.clear()
        }
      }

      private suspend fun updateSpeed(speed: Long) {
        val expected = mCurrentTask
        val backend = mCurrentBackend
        withExpectedDownloadProgressTask(expected, backend) { info, selectedBackend ->
          val transfers = snapshotTransfers()
          applyDownloadProgress(
            info,
            DownloadProgressCommand.estimate(speed, transfers),
            selectedBackend,
          )
          mDownloadListener?.onDownload(info)
          mutableNotifyFlow.emit(info)
        }
      }

      private suspend fun snapshotTransfers() = mutex.withLock {
        (0 until contentLengthMap.size).map { index ->
          com.hippo.ehviewer.bridge.download.ActiveDownloadTransfer(
            contentLengthMap.valueAt(index),
            receivedSizeMap.valueAt(index),
          )
        }
      }
    }

  private const val TAG = "DownloadManager"
}

var downloadLocation: Path
  get() =
    with(Settings) {
      if (downloadScheme != null) {
        Uri.Builder()
          .apply {
            scheme(downloadScheme)
            encodedAuthority(downloadAuthority)
            encodedPath(downloadPath)
            encodedQuery(downloadQuery)
            encodedFragment(downloadFragment)
          }
          .build()
          .toOkioPath()
      } else {
        AppConfig.defaultDownloadDir?.toOkioPath() ?: "".toPath()
      }
    }
  set(value) =
    with(value.toUri()) {
      Settings.edit {
        downloadScheme = scheme
        downloadAuthority = encodedAuthority
        downloadPath = encodedPath
        downloadQuery = encodedQuery
        downloadFragment = encodedFragment
      }
    }

val DownloadInfo.downloadDir
  get() = dirname?.let { downloadLocation / it }
val DownloadInfo.archiveFile
  get() = downloadDir?.run { find("$gid.cbz") ?: find("$gid.zip") }
val GalleryInfo.tempDownloadDir
  get() = AppConfig.externalTempPersistDir?.let { it / "$gid" }
