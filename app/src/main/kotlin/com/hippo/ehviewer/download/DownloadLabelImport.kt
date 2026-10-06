package com.hippo.ehviewer.download

import com.ehviewer.core.database.model.DownloadLabel

/** Appends unique imported names after every existing position without modifying source rows. */
internal fun planDownloadLabelImport(
  current: List<DownloadLabel>,
  imported: List<DownloadLabel>,
): List<DownloadLabel> {
  val names = current.mapTo(HashSet()) { it.label }
  val additions = imported.filter { names.add(it.label) }
  val firstPosition = ((current.maxOfOrNull { it.position } ?: -1).toLong() + 1).coerceAtLeast(0)
  require(firstPosition + additions.size <= Int.MAX_VALUE.toLong() + 1) {
    "Download label positions exceed Int range"
  }
  return additions.mapIndexed { index, label ->
    label.copy(position = (firstPosition + index).toInt(), id = null)
  }
}
