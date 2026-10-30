/*
 * Copyright 2023-2024 Tarsin Norbin
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
package com.hippo.ehviewer.gallery

import arrow.autoCloseScope
import com.ehviewer.core.files.openFileDescriptor
import com.ehviewer.core.model.GalleryInfo
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.Settings.archivePasswds
import com.hippo.ehviewer.bridge.archive.RustArchiveReaderV2
import com.hippo.ehviewer.bridge.reader.readerArchiveSorting
import com.hippo.ehviewer.client.EhUtils
import com.hippo.ehviewer.image.ImageSource
import com.hippo.ehviewer.image.byteBufferSource
import com.hippo.ehviewer.util.FileUtils
import com.hippo.ehviewer.util.displayName
import kotlinx.coroutines.coroutineScope
import moe.tarsin.kt.install
import okio.Path

typealias PasswdInvalidator = (String) -> Boolean

typealias PasswdProvider = suspend (PasswdInvalidator) -> String

suspend fun <T> useArchivePageLoader(
  file: Path,
  info: GalleryInfo? = null,
  startPage: Int = 0,
  hasAds: Boolean = false,
  passwdProvider: PasswdProvider,
  block: suspend (PageLoader) -> T,
) = autoCloseScope {
  coroutineScope {
    val pfd = install(file.openFileDescriptor("r"))
    val sortEntries = readerArchiveSorting(info != null, file.name)
    val archiveReader =
      install(
        openArchiveReader(
          fileDescriptor = pfd.fd,
          sortEntries = sortEntries,
        )
      )
    if (archiveReader.needsPassword && archivePasswds.none(archiveReader::providePassword)) {
      archivePasswds += passwdProvider(archiveReader::providePassword)
    }
    val size = archiveReader.entryCount
    check(size > 0) { "Archive have no content!" }
    val loader =
      install(
        object : PageLoader(this, info, startPage, size, hasAds) {
          override val title by lazy {
            if (info != null) {
              EhUtils.getSuitableTitle(info)
            } else {
              FileUtils.getNameFromFilename(file.displayName)!!
            }
          }

          override fun getImageExtension(index: Int) = archiveReader.extension(index)

          override fun save(index: Int, file: Path) = runCatching {
            file.openFileDescriptor("w").use {
              archiveReader.extractToFileDescriptor(index, it.fd)
            }
          }
            .getOrElse {
              logcat(it)
              false
            }

          override fun openSource(index: Int): ImageSource {
            return archiveReader.openSource(index)
          }

          override fun prefetchPages(pages: List<Int>, bounds: IntRange) = Unit

          override fun onRequest(index: Int, force: Boolean, orgImg: Boolean) =
            notifySourceReady(index)
        }
      )
    block(loader)
  }
}

private interface ArchiveReader : AutoCloseable {
  val entryCount: Int
  val needsPassword: Boolean

  fun providePassword(password: String): Boolean

  fun extension(index: Int): String

  fun extractToFileDescriptor(index: Int, fileDescriptor: Int): Boolean

  fun openSource(index: Int): ImageSource
}

private class RustArchiveReader(private val delegate: RustArchiveReaderV2) : ArchiveReader {
  override val entryCount
    get() = delegate.entryCount

  override val needsPassword
    get() = delegate.needsPassword

  override fun providePassword(password: String) = delegate.providePassword(password)

  override fun extension(index: Int) = delegate.extension(index)

  override fun extractToFileDescriptor(index: Int, fileDescriptor: Int) =
    delegate.extractToFileDescriptor(index, fileDescriptor)

  override fun openSource(index: Int): ImageSource {
    val buffer = delegate.extract(index)
    check(buffer.isDirect)
    return byteBufferSource(buffer) {}
  }

  override fun close() = delegate.close()
}

private fun openArchiveReader(
  fileDescriptor: Int,
  sortEntries: Boolean,
): ArchiveReader = RustArchiveReader(RustArchiveReaderV2.open(fileDescriptor, sortEntries))
