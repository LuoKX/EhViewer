package com.hippo.ehviewer.updater

import io.ktor.utils.io.ByteChannel
import io.ktor.utils.io.ByteReadChannel
import java.io.File
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext

class UpdatePackageDownloadTest {
  private fun fixture(block: suspend (File, File) -> Unit) = runBlocking {
    val dir = kotlin.io.path.createTempDirectory("update-download-test").toFile()
    try {
      val target = File(dir, "update.apk").apply { writeText("old") }
      block(dir, target)
      assertEquals(listOf("update.apk"), dir.list()!!.toList())
    } finally {
      dir.deleteRecursively()
    }
  }

  @Test
  fun directDownloadIsPublishedAfterCompletion() = fixture { _, target ->
    val bytes = "apk".encodeToByteArray()
    stageUpdateDownload(ByteReadChannel(bytes), target, false) { _, _ -> error("unpack called") }
    assertContentEquals(bytes, target.readBytes())
  }

  @Test
  fun zipStageUsesCompleteInputAndPublishesOnlyExtractedBytes() = fixture { _, target ->
    var calls = 0
    stageUpdateDownload(ByteReadChannel("zip".encodeToByteArray()), target, true) { source, output
      ->
      calls++
      assertEquals("zip", source.readText())
      assertEquals("old", target.readText())
      output.writeText("apk")
    }
    assertEquals(1, calls)
    assertEquals("apk", target.readText())
  }

  @Test
  fun emptyOversizedAndFailedExtractionKeepExistingTarget() = fixture { _, target ->
    assertFailsWith<IOException> {
      stageUpdateDownload(ByteReadChannel(byteArrayOf()), target, false)
    }
    assertFailsWith<IOException> {
      stageUpdateDownload(ByteReadChannel(ByteArray(5)), target, false, maximumBytes = 4)
    }
    assertFailsWith<IOException> {
      stageUpdateDownload(ByteReadChannel(byteArrayOf(1)), target, true) { _, output ->
        output.writeText("partial")
        throw IOException("fixture")
      }
    }
    assertFailsWith<IOException> {
      stageUpdateDownload(ByteReadChannel(byteArrayOf(1)), target, true) { _, _ -> }
    }
    assertEquals("old", target.readText())
  }

  @Test
  fun cancellationDoesNotPublishOrReplayExtraction() = fixture { _, target ->
    var calls = 0
    assertFailsWith<CancellationException> {
      stageUpdateDownload(ByteReadChannel(byteArrayOf(1)), target, true) { _, output ->
        calls++
        output.writeText("partial")
        throw CancellationException("fixture")
      }
    }
    assertEquals(1, calls)
    assertEquals("old", target.readText())
  }

  @Test
  fun cancellationAfterExtractionStillPreservesTheTarget() = fixture { _, target ->
    val job = Job()
    assertFailsWith<CancellationException> {
      withContext(job) {
        stageUpdateDownload(ByteReadChannel(byteArrayOf(1)), target, true) { _, output ->
          output.writeText("complete but cancelled")
          job.cancel()
        }
      }
    }
    assertEquals("old", target.readText())
  }

  @Test
  fun failedNetworkAndPublicationCleanOnlyOwnedStagingFiles() = fixture { _, target ->
    val failed = ByteChannel().apply { cancel(IOException("fixture channel")) }
    assertFailsWith<IOException> { stageUpdateDownload(failed, target, false) }
    assertEquals("old", target.readText())
    check(target.delete() && target.mkdir())
    assertFailsWith<IOException> {
      stageUpdateDownload(ByteReadChannel(byteArrayOf(1)), target, false)
    }
    check(target.isDirectory)
  }
}
