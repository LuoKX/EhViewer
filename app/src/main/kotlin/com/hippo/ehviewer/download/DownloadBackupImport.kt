package com.hippo.ehviewer.download

import com.ehviewer.core.database.model.DownloadInfo
import com.hippo.ehviewer.bridge.download.BackupImportPlan
import com.hippo.ehviewer.bridge.download.BackupImportRequest
import com.hippo.ehviewer.bridge.download.ImportDownload
import com.hippo.ehviewer.bridge.download.planBackupImport

/** Room wrappers/state copied before planning; backup inputs never normalized in place. */
internal fun planDownloadImport(
  current: Collection<Long>,
  imported: List<DownloadInfo>,
  planner: (BackupImportRequest) -> BackupImportPlan = { planBackupImport(it) },
): List<DownloadInfo> {
  val currentGids = current.toList()
  val sources = imported.toList()
  val sourceFacts = sources.map { ImportDownload(it.gid, it.state) }
  val sourceCopies = sources.map { it.copy(downloadInfo = it.downloadInfo.copy()) }
  val request = BackupImportRequest(1, currentGids, sourceFacts, emptyList(), emptyList())
  val plan = planner(request)
  require(
    current.toList() == currentGids &&
      imported.size == sources.size &&
      imported.withIndex().all { (index, row) ->
        row === sources[index] &&
          row.gid == sourceFacts[index].gid &&
          row.state == sourceFacts[index].state
      }
  ) {
    "Backup download snapshot changed"
  }
  return plan.downloads.map { row ->
    sourceCopies[row.sourceIndex].copy(
      downloadInfo = sourceCopies[row.sourceIndex].downloadInfo.copy(state = row.state)
    )
  }
}
