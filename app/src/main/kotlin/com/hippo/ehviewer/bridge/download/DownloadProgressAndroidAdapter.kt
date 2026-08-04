package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo
import com.hippo.ehviewer.bridge.CoreBackend

/** Returns true only for the same non-null Android task captured at callback receipt. */
internal fun isExpectedDownloadProgressTask(
  current: Any?,
  expected: Any?,
): Boolean = expected != null && current === expected

/** Builds an immutable snapshot from Android-owned runtime fields. */
internal fun DownloadInfo.toDownloadProgressSnapshot(): DownloadProgressSnapshot =
  DownloadProgressSnapshot(
    state.toRustDownloadState(),
    speed,
    remaining,
    finished,
    downloaded,
    total,
    legacy,
  )

/** Legacy planner used only when the operation selected the legacy backend. */
internal fun planLegacyDownloadProgress(
  snapshot: DownloadProgressSnapshot,
  command: DownloadProgressCommand,
): DownloadProgressPlan {
  var result = snapshot
  when (command.kind) {
    DownloadProgressCommandKind.RESET ->
      result =
        result.copy(
          state = DownloadState.DOWNLOADING,
          speedBytesPerSecond = -1,
          remainingMilliseconds = -1,
          total = -1,
          finished = 0,
          downloaded = 0,
          legacy = -1,
        )
    DownloadProgressCommandKind.SET_PAGES ->
      result = result.copy(total = requireNotNull(command.total))
    DownloadProgressCommandKind.SET_PAGE_RESULT ->
      result =
        result.copy(
          finished = requireNotNull(command.finished),
          downloaded = requireNotNull(command.downloaded),
          total = requireNotNull(command.total),
        )
    DownloadProgressCommandKind.ESTIMATE -> {
      val speed = requireNotNull(command.speedBytesPerSecond)
      val transfers = requireNotNull(command.transfers)
      val remaining =
        when {
          result.total <= 0 -> -1
          speed == 0L -> 300L * 24L * 60L * 60L * 1000L
          transfers.isEmpty() -> result.remainingMilliseconds
          else -> {
            var count = 0L
            var contentSum = 0L
            var totalSize = 0L
            transfers.forEach { transfer ->
              count++
              contentSum += transfer.contentLength
              totalSize += transfer.contentLength - transfer.receivedSize
            }
            totalSize += contentSum * (result.total - result.downloaded - count.toInt()) / count
            totalSize / speed * 1000
          }
        }
      result = result.copy(speedBytesPerSecond = speed, remainingMilliseconds = remaining)
    }
    DownloadProgressCommandKind.FINISH ->
      result =
        result.copy(
          finished = requireNotNull(command.finished),
          downloaded = requireNotNull(command.downloaded),
          total = requireNotNull(command.total),
          legacy = requireNotNull(command.total).wrappingSub(requireNotNull(command.finished)),
          state =
            if (requireNotNull(command.failed)) DownloadState.FAILED else DownloadState.FINISHED,
        )
  }
  return DownloadProgressPlan(result)
}

/** Runs exactly one planner for the backend captured by the owning operation. */
internal fun selectDownloadProgressPlan(
  backend: CoreBackend,
  legacyPlanner: () -> DownloadProgressPlan,
  rustPlanner: () -> DownloadProgressPlan,
): DownloadProgressPlan =
  when (backend) {
    CoreBackend.LEGACY -> legacyPlanner()
    CoreBackend.RUST -> rustPlanner()
  }

private fun Int.wrappingSub(other: Int): Int = this - other
