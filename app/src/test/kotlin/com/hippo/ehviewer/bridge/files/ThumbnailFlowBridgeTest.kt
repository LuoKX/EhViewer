package com.hippo.ehviewer.bridge.files

import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class ThumbnailFlowBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun activationAndMetadataWritePrecedeSuffixValidation() = runBlocking {
    for (info in listOf(false, true)) for (blank in listOf(false, true)) assertEquals(
      info && !blank,
      usesDownloadedThumbnail(info, blank),
    )
    for (key in listOf("a.jpg", "a.b.PNG", "a. x ", "a.\uD800", "a.", "none", "a.\u00a0　")) {
      val events = mutableListOf<String>()
      val expected = key.substringAfterLast('.', "")
      val result = runCatching {
        prepareDownloadedThumbnail(
          "old",
          key,
          { events += "update" },
          {
            events += "dir"
            "directory"
          },
        )
      }
      assertEquals(listOf("update", "dir"), events)
      if (expected.isNotBlank()) assertEquals("directory" to "thumb.$expected", result.getOrThrow())
      else assertIs<IllegalStateException>(result.exceptionOrNull())
    }
    assertEquals(
      1 to "thumb.jpg",
      prepareDownloadedThumbnail("a.jpg", "a.jpg", { fail("unchanged key") }, { 1 }),
    )
    assertFailsWith<IOException> {
      prepareDownloadedThumbnail(
        "old",
        "a.jpg",
        { throw IOException("write") },
        { fail("directory after DB failure") },
      )
    }
    Unit
  }

  private suspend fun runFlow(
    events: MutableList<String>,
    file: Boolean,
    localSuccess: Boolean,
    remoteSuccess: Boolean,
    dir: Boolean,
    different: Boolean,
    cacheHit: Boolean = true,
    failure: Pair<String, Throwable>? = null,
  ): String {
    fun event(name: String) {
      events += name
      if (failure?.first == name) throw failure.second
    }
    return executeDownloadedThumbnail(
      localExists = {
        event("file")
        file
      },
      local = {
        event("local")
        if (localSuccess) "local-ok" else "local-error"
      },
      remote = {
        event("remote")
        if (remoteSuccess) "remote-ok" else "remote-error"
      },
      success = { it.endsWith("ok") },
      directoryExists = {
        event("dir")
        dir
      },
      differentOldPath = {
        event("different")
        different
      },
      writeCache = {
        event("cache")
        if (cacheHit) event("overwrite")
      },
      deleteOld = { event("delete") },
    )
  }

  @Test
  fun fullEffectMatrixPreservesCacheMissCleanupDirectoryVanishingAndSameJpeg() = runBlocking {
    for (file in listOf(false, true)) for (local in listOf(false, true)) for (remote in
      listOf(false, true)) for (dir in listOf(false, true)) for (different in
      listOf(false, true)) for (cache in listOf(false, true)) {
      val events = mutableListOf<String>()
      val result = runFlow(events, file, local, remote, dir, different, cache)
      val expected = buildList {
        add("file")
        if (file) {
          add("local")
          if (local) {
            add("different")
            if (different) add("delete")
          }
        }
        if (!file || !local) {
          add("remote")
          if (remote) {
            add("dir")
            if (dir) {
              add("cache")
              if (cache) add("overwrite")
              add("different")
              if (different) add("delete")
            }
          }
        }
      }
      assertEquals(expected, events)
      assertEquals(
        if (file && local) "local-ok" else if (remote) "remote-ok" else "remote-error",
        result,
      )
    }
  }

  @Test
  fun everyFailureAndCancellationStopsAtTheOriginalEffect() = runBlocking {
    val baseline =
      listOf("file", "local", "remote", "dir", "cache", "overwrite", "different", "delete")
    for (step in baseline) for (error in listOf(IOException(step), CancellationException(step))) {
      val events = mutableListOf<String>()
      assertSame(
        error,
        assertFails { runFlow(events, true, false, true, true, true, failure = step to error) },
      )
      assertEquals(baseline.take(baseline.indexOf(step) + 1), events)
    }
    val events = mutableListOf<String>()
    val error = IOException("local cleanup")
    assertSame(
      error,
      assertFails { runFlow(events, true, true, true, true, true, failure = "delete" to error) },
    )
    assertEquals(listOf("file", "local", "different", "delete"), events)
  }

  @Test
  fun invalidNativeIntentsAndRawTextAreRejected() {
    assertFails { thumbnailPlan(2, intArrayOf(1, 65536)) }
    assertFails { thumbnailPlan(0, intArrayOf(2, 0)) }
    for ((phase, bad) in
      listOf(
        0 to intArrayOf(2),
        1 to intArrayOf(),
        2 to intArrayOf(99),
        3 to intArrayOf(2, 0),
        4 to intArrayOf(0, 1),
        5 to intArrayOf(3, 1),
        6 to intArrayOf(3, 0),
      )) {
      assertFails { thumbnailPlan(phase, intArrayOf(1)) { _, _ -> bad } }
    }
  }
}
