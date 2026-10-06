/*
 * Copyright (C) 2015 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.util

import com.ehviewer.core.files.write
import com.hippo.ehviewer.bridge.files.FileRuleKind
import com.hippo.ehviewer.bridge.files.StorageOperation
import com.hippo.ehviewer.bridge.files.StorageRulesRequest
import com.hippo.ehviewer.bridge.files.planFileRules
import com.hippo.ehviewer.bridge.files.planStorageRules
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.InternalAPI
import io.ktor.utils.io.rethrowCloseCauseIfNeeded
import java.io.File
import okio.Path

object FileUtils {
  fun ensureDirectory(file: File?) =
    file?.let { if (it.exists()) it.isDirectory else it.mkdirs() } == true

  /**
   * Convert byte to human readable string.
   *
   * @param bytes the bytes to convert
   * @return the human readable string
   */
  fun humanReadableByteCount(bytes: Long): String {
    val units = "KMGTPE"
    var exp = -1
    var result = bytes.toDouble()
    while (result >= 1024.0 && exp < units.length - 1) {
      exp++
      result /= 1024.0
    }
    return if (exp == -1) "$bytes B" else "%.1f %siB".format(result, units[exp])
  }

  fun cleanupDirectory(dir: File?, maxFiles: Int = 10) {
    dir?.listFiles()?.let { files ->
      val plan =
        planStorageRules(
          StorageRulesRequest(
            operation = StorageOperation.CLEANUP,
            modified = files.map { it.lastModified() },
            maxItems = maxFiles,
          )
        )
      // Rust selects original array slots; Android retains non-recursive delete semantics.
      plan.indices.forEach { files[it].delete() }
    }
  }

  /** Rust owns FAT character rules, UTF-8 budgets and middle shortening. */
  fun sanitizeFilename(name: String): String =
    checkNotNull(planFileRules(FileRuleKind.SANITIZE, name))

  fun getExtensionFromFilename(filename: String?) = planFileRules(FileRuleKind.EXTENSION, filename)

  fun getNameFromFilename(filename: String?) = planFileRules(FileRuleKind.BASENAME, filename)
}

@OptIn(InternalAPI::class)
suspend fun ByteReadChannel.copyTo(file: Path) {
  file.write {
    while (!isClosedForRead) {
      transferFrom(readBuffer)
      awaitContent()
    }
    rethrowCloseCauseIfNeeded()
  }
}
