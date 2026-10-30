package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo

/** Apply only to fresh Room query objects, never the live worker queue. No writes occur here. */
internal fun projectDownloadStartupStates(
  rows: List<DownloadInfo>,
  nativeCall: (IntArray) -> IntArray = ::nativeNormalizeDownloadStatesV1,
): List<DownloadInfo> {
  require(rows.size <= 100_000) { "Download state snapshot exceeds limit" }
  val before = IntArray(rows.size) { rows[it].state }
  val after = nativeCall(before.copyOf())
  require(
    after.size == before.size &&
      after.indices.all {
        if (before[it] in DownloadInfo.STATE_WAIT..DownloadInfo.STATE_DOWNLOAD)
          after[it] == DownloadInfo.STATE_NONE
        else after[it] == before[it]
      }
  ) {
    "Invalid Rust download state projection"
  }
  // Validate the entire response before mutating any of the detached query rows.
  rows.forEachIndexed { index, row -> row.state = after[index] }
  return rows
}

private external fun nativeNormalizeDownloadStatesV1(states: IntArray): IntArray
