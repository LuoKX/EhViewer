package com.hippo.ehviewer.gallery

import arrow.autoCloseScope
import com.ehviewer.core.files.list
import com.ehviewer.core.files.sendTo
import com.hippo.ehviewer.bridge.reader.LocalPagesRequest
import com.hippo.ehviewer.bridge.reader.selectLocalPages
import com.hippo.ehviewer.image.ImageSource
import com.hippo.ehviewer.image.PathSource
import kotlinx.coroutines.coroutineScope
import moe.tarsin.kt.install
import okio.Path

/**
 * Opens a staged recovery directory without inventing a remote gallery identity.
 *
 * Android owns directory enumeration and file access. Page scheduling remains in [PageLoader]'s
 * Rust planner, while the strict eight-digit filename policy matches the Rust recovery contract.
 */
suspend fun <T> useLocalDirectoryPageLoader(
  directory: Path,
  title: String,
  startPage: Int = 0,
  block: suspend (PageLoader) -> T,
) = autoCloseScope {
  coroutineScope {
    val entries = directory.list()
    val selected = selectLocalPages(LocalPagesRequest(names = entries.map { it.name }))
    val files = selected.map { entries[it.sourceIndex] }
    check(files.isNotEmpty()) { "Local recovery directory has no readable pages" }
    val loader =
      install(
        object : PageLoader(this, null, startPage, files.size, false) {
          override val title = title

          override fun getImageExtension(index: Int) = selected[index].extension

          override fun save(index: Int, file: Path) = runCatching {
            files[index] sendTo file
            true
          }
            .getOrDefault(false)

          override fun openSource(index: Int): ImageSource =
            object : PathSource {
              override val source = files[index]
              override val type = checkNotNull(getImageExtension(index))

              override fun close() = Unit
            }

          override fun prefetchPages(pages: List<Int>, bounds: IntRange) = Unit

          override fun onRequest(index: Int, force: Boolean, orgImg: Boolean) =
            notifySourceReady(index)
        }
      )
    block(loader)
  }
}
