/*
 * Copyright 2019 Hippo Seven
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
package com.hippo.ehviewer.client

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import arrow.core.Either
import com.ehviewer.core.files.delete
import com.ehviewer.core.files.exists
import com.ehviewer.core.files.metadataOrNull
import com.ehviewer.core.files.moveTo
import com.ehviewer.core.files.openFileDescriptor
import com.ehviewer.core.files.write
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.EhApplication.Companion.ktorClient
import com.hippo.ehviewer.R
import com.hippo.ehviewer.bridge.tags.TagSuggestionRequest
import com.hippo.ehviewer.bridge.tags.TagTranslationIndex
import com.hippo.ehviewer.util.AppConfig
import com.hippo.ehviewer.util.bodyAsUtf8Text
import com.hippo.ehviewer.util.ensureSuccess
import com.hippo.ehviewer.util.utf8
import io.ktor.client.request.prepareGet
import io.ktor.client.statement.bodyAsChannel
import io.ktor.utils.io.readAvailable
import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write
import kotlin.time.Clock
import kotlin.time.Duration.Companion.days
import kotlin.time.Instant
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.io.writeString
import moe.tarsin.coroutines.runSuspendCatching
import okio.Path
import splitties.init.appCtx

object EhTagDatabase : CoroutineScope {
  override val coroutineContext = Dispatchers.IO + Job()
  private val updateLock = Mutex()
  private val indexLock = ReentrantReadWriteLock()
  private var index: TagTranslationIndex? = null
  var initialized by mutableStateOf(false)
    private set

  fun getTranslation(prefix: String? = "n", tag: String?): String? =
    if (prefix == null || tag == null) null else indexLock.read { index?.translate(prefix, tag) }

  data class Tag(val tag: String, val hint: String?, val score: Float)

  context(_: Context)
  fun suggestion(rawKeyword: String, expectTranslate: Boolean): List<Tag> = indexLock.read {
    index
      ?.suggest(
        TagSuggestionRequest(
          version = 1,
          keyword = rawKeyword,
          translate = expectTranslate && translatable,
          limit = 50,
        )
      )
      ?.map { Tag(it.tag, it.hint, it.score) }
      .orEmpty()
  }

  private fun metadata(context: Context): Array<String> =
    context.resources.getStringArray(R.array.tag_translation_metadata)

  context(ctx: Context)
  val translatable
    get() = ctx.resources.getBoolean(R.bool.tag_translatable)

  private suspend fun fetch(url: String) =
    ktorClient.prepareGet(url).executeSafely {
      it.status.ensureSuccess()
      it.bodyAsUtf8Text()
    }

  // Stream public JSON into a staging file. Never allocate its full text on the Java heap.
  private suspend fun fetchData(url: String, target: Path) =
    ktorClient.prepareGet(url).executeSafely { response ->
      response.status.ensureSuccess()
      val channel = response.bodyAsChannel()
      val buffer = ByteArray(16 * 1024)
      var total = 0L
      target.write {
        while (true) {
          val count = channel.readAvailable(buffer)
          if (count == -1) break
          total += count
          require(total <= MAX_TAG_DATA_BYTES) { "Tag index input exceeds resource budget" }
          write(buffer, 0, count)
        }
      }
    }

  fun launchUpdate() = launch {
    updateLock.withLock {
      runSuspendCatching { atomicallyUpdate() }.onFailure { logcat(it) }
    }
  }

  private fun openIndex(path: Path, remote: Boolean) =
    path.openFileDescriptor("r").use { TagTranslationIndex.open(it.fd, remote) }

  private fun publish(replacement: TagTranslationIndex) {
    val previous = indexLock.write {
      val previous = index
      index = replacement
      initialized = true
      previous
    }
    previous?.close()
  }

  private suspend fun atomicallyUpdate() {
    val (sha1Name, sha1Url, dataName, dataUrl) = metadata(appCtx)
    val workdir = AppConfig.tagTranslationsDir
    val sha1File = workdir / sha1Name
    val legacyData = workdir / dataName
    // Preserve the old cached map for compatibility. New cache contains the original remote JSON.
    val dataFile = workdir / "$dataName.rust-v1"
    if (!initialized) {
      Either.catch {
          val remote = dataFile.exists()
          val source = if (remote) dataFile else legacyData
          publish(openIndex(source, remote))
          val modified = source.metadataOrNull()?.lastModifiedAtMillis
          if (
            modified != null &&
              Clock.System.now() - Instant.fromEpochMilliseconds(modified) < 1.days
          )
            return
        }
        .onLeft { logcat(it) }
    }
    val currentSha1 = if (initialized) Either.catch { sha1File.utf8() }.getOrNull() else null
    val newSha1 = fetch(sha1Url)
    if (newSha1 == currentSha1) return
    val staging = workdir / "$dataName.rust-v1.tmp"
    val stagingSha = workdir / "$sha1Name.tmp"
    var replacement: TagTranslationIndex? = null
    try {
      fetchData(dataUrl, staging)
      replacement = openIndex(staging, remote = true)
      stagingSha.write { writeString(newSha1) }
      staging moveTo dataFile
      stagingSha moveTo sha1File
      val accepted = checkNotNull(replacement)
      replacement = null // Publication owns it even if retiring the previous handle fails.
      publish(accepted)
    } finally {
      replacement?.close()
      staging.delete()
      stagingSha.delete()
    }
  }

  private const val MAX_TAG_DATA_BYTES = 64L * 1024 * 1024
}
