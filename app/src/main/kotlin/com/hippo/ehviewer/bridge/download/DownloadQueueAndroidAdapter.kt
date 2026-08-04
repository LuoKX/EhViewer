package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo

/** Converts one legacy Android download-state constant into the stable Rust wire state. */
internal fun Int.toRustDownloadState(): DownloadState =
  when (this) {
    DownloadInfo.STATE_INVALID -> DownloadState.INVALID
    DownloadInfo.STATE_NONE -> DownloadState.NONE
    DownloadInfo.STATE_WAIT -> DownloadState.WAITING
    DownloadInfo.STATE_DOWNLOAD -> DownloadState.DOWNLOADING
    DownloadInfo.STATE_FINISH -> DownloadState.FINISHED
    DownloadInfo.STATE_FAILED -> DownloadState.FAILED
    else -> throw IllegalArgumentException("Unsupported Android download state: $this")
  }

/** Converts one stable Rust wire state back into the legacy Android database constant. */
internal fun DownloadState.toAndroidDownloadState(): Int =
  when (this) {
    DownloadState.INVALID -> DownloadInfo.STATE_INVALID
    DownloadState.NONE -> DownloadInfo.STATE_NONE
    DownloadState.WAITING -> DownloadInfo.STATE_WAIT
    DownloadState.DOWNLOADING -> DownloadInfo.STATE_DOWNLOAD
    DownloadState.FINISHED -> DownloadInfo.STATE_FINISH
    DownloadState.FAILED -> DownloadInfo.STATE_FAILED
  }

/** Builds the complete queue snapshot consumed by the pure Rust reducer. */
internal fun buildDownloadQueueSnapshot(
  entries: List<DownloadInfo>,
  waiting: Collection<DownloadInfo>,
  current: DownloadInfo?,
) =
  DownloadQueueSnapshot(
    entries = entries.map { DownloadEntry(it.gid, it.state.toRustDownloadState()) },
    waiting = waiting.map(DownloadInfo::gid),
    current = current?.gid,
  )

/**
 * Finds persisted volatile states that are not represented by Android's live worker topology.
 *
 * These entries can survive process death or a live rollback from the legacy scheduler. Rust
 * recovery resets only this returned subset, so a valid active task and FIFO remain untouched.
 */
internal fun buildDownloadQueueRecoveryEntries(
  entries: List<DownloadInfo>,
  waiting: Collection<DownloadInfo>,
  current: DownloadInfo?,
): List<DownloadEntry> {
  val currentGalleryId = current?.gid
  val waitingGalleryIds = waiting.mapTo(mutableSetOf(), DownloadInfo::gid)
  return entries.mapNotNull { info ->
    val state = info.state.toRustDownloadState()
    val isOrphan =
      when (state) {
        DownloadState.WAITING -> info.gid !in waitingGalleryIds
        DownloadState.DOWNLOADING -> info.gid != currentGalleryId
        else -> false
      }
    DownloadEntry(info.gid, state).takeIf { isOrphan }
  }
}
