package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo

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
