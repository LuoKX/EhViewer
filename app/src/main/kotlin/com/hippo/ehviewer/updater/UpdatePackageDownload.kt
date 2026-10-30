package com.hippo.ehviewer.updater

import com.hippo.ehviewer.bridge.updater.extractUpdateApk
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.readAvailable
import java.io.File
import java.io.IOException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal const val MAX_UPDATE_BYTES = 512L * 1024 * 1024

/** Publish only a complete download/extraction, using owned files beside the target. */
internal suspend fun stageUpdateDownload(
  channel: ByteReadChannel,
  target: File,
  zip: Boolean,
  maximumBytes: Long = MAX_UPDATE_BYTES,
  unpack: (File, File) -> Unit = ::extractUpdateApk,
) {
  require(maximumBytes in 1..MAX_UPDATE_BYTES) { "Invalid update download limit" }
  val directory = requireNotNull(target.absoluteFile.parentFile)
  val download = File.createTempFile("update-download-", ".tmp", directory)
  var extracted: File? = null
  try {
    download.outputStream().use { output ->
      val bytes = ByteArray(64 * 1024)
      var count = 0L
      while (true) {
        currentCoroutineContext().ensureActive()
        val read = channel.readAvailable(bytes, 0, bytes.size)
        if (read < 0) break
        if (read == 0) {
          channel.awaitContent()
          continue
        }
        count += read
        if (count > maximumBytes) throw IOException("Update download exceeds limit")
        output.write(bytes, 0, read)
      }
      channel.closedCause?.let { throw it }
      if (count == 0L) throw IOException("Empty update download")
      output.fd.sync()
    }
    currentCoroutineContext().ensureActive()
    val complete =
      if (zip) {
        File.createTempFile("update-apk-", ".tmp", directory).also {
          extracted = it
          unpack(download, it)
          if (it.length() !in 1..MAX_UPDATE_BYTES)
            throw IOException("Invalid extracted update size")
        }
      } else download
    currentCoroutineContext().ensureActive()
    if (!complete.renameTo(target)) throw IOException("Unable to publish update download")
  } finally {
    extracted?.delete()
    download.delete()
  }
}
