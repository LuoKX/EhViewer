package com.hippo.ehviewer.download

import com.ehviewer.core.database.model.DownloadLabel
import com.hippo.ehviewer.bridge.download.BackupImportPlan
import com.hippo.ehviewer.bridge.download.BackupImportRequest
import com.hippo.ehviewer.bridge.download.importLabel
import com.hippo.ehviewer.bridge.download.planBackupImport

/**
 * Rust owns repaired merge policy; Android only snapshots and copies Room rows with cleared IDs.
 */
internal fun planDownloadLabelImport(
  current: List<DownloadLabel>,
  imported: List<DownloadLabel>,
  planner: (BackupImportRequest) -> BackupImportPlan = { planBackupImport(it) },
): List<DownloadLabel> {
  val currentSnapshot = current.map { it.copy() }
  val sourceSnapshot = imported.map { it.copy() }
  val request =
    BackupImportRequest(
      1,
      emptyList(),
      emptyList(),
      currentSnapshot.map { importLabel(it.label, it.position) },
      sourceSnapshot.map { importLabel(it.label, it.position) },
    )
  val plan = planner(request)
  require(current == currentSnapshot && imported == sourceSnapshot) {
    "Backup label snapshot changed"
  }
  return plan.labels.map { row ->
    sourceSnapshot[row.sourceIndex].copy(position = row.position, id = null)
  }
}
