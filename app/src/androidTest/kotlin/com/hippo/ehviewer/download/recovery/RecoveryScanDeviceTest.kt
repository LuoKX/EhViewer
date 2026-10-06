package com.hippo.ehviewer.download.recovery

import android.net.Uri
import android.system.Os
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.files.*
import com.hippo.ehviewer.util.FileUtils
import java.io.File
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real JNI/physical effects. Compiling sources alone is not device runtime acceptance. */
@RunWith(AndroidJUnit4::class)
class RecoveryScanDeviceTest {
  private val context
    get() = InstrumentationRegistry.getInstrumentation().targetContext

  private fun fixture() =
    File(context.cacheDir, "recovery-scan-${UUID.randomUUID()}").apply {
      check(mkdir())
    }

  @Test
  fun physicalWalkKeepsBfsDepthAndSymlinkBehavior() = runBlocking {
    val root = fixture()
    val link = File(root, "link")
    try {
      File(root, "a").mkdir()
      File(root, "b").mkdir()
      File(root, "a/deep").mkdir()
      File(root, "a/00000001.jpg").writeText("page")
      File(root, "b/00000001.jpg").writeText("page")
      File(root, "z").writeText("file")
      Os.symlink(File(root, "a").path, link.path)
      val counts = mutableListOf<Int>()
      val tree =
        SafTreeWalker.walk(context, Uri.fromFile(root), RecoveryScanLimits(maxDepth = 1)) {
          counts += it
        }
      assertEquals(
        listOf("", "a", "b", "z", "a/00000001.jpg", "b/00000001.jpg"),
        tree.nodes.map { it.relativePath },
      )
      assertEquals((1..6).toList(), counts)
      assertTrue(tree.limitExceeded)
    } finally {
      link.delete()
      root.deleteRecursively()
    }
  }

  @Test
  fun cancellationClosesNativeScanAndFreesAllFourRegistrySlots() = runBlocking {
    val root = fixture()
    try {
      File(root, "child").writeText("page")
      val cancelled = CancellationException("fixture")
      try {
        SafTreeWalker.walk(context, Uri.fromFile(root), RecoveryScanLimits()) {
          if (it == 2) throw cancelled
        }
        fail("Cancellation swallowed")
      } catch (actual: CancellationException) {
        assertSame(cancelled, actual)
      }
      val scans = mutableListOf<RecoveryScanSession>()
      try {
        repeat(4) {
          scans +=
            RecoveryScanSession.open(
              RecoveryScanOptions(
                backend = RecoveryScanBackend.PHYSICAL,
                maxDepth = 1,
                maxNodes = 4,
                startedAtMillis = 0,
                maxDurationMillis = 1000,
                rootDocumentId = byteArrayOf(),
              )
            )
        }
      } finally {
        scans.forEach { it.close() }
      }
    } finally {
      root.deleteRecursively()
    }
  }

  @Test
  fun cleanupAndCandidatePlansPreserveStableOrderingAndNonRecursiveDeletion() {
    val root = fixture()
    try {
      File(root, "a").apply {
        writeText("a")
        setLastModified(1000)
      }
      File(root, "b").apply {
        writeText("b")
        setLastModified(2000)
      }
      File(root, "c").apply {
        writeText("c")
        setLastModified(2000)
      }
      File(root, "nested").apply {
        mkdir()
        File(this, "keep").writeText("keep")
        setLastModified(0)
      }
      val original = root.listFiles()!!.toList()
      val retained =
        original.sortedByDescending { it.lastModified() }.take(2).map { it.name }.toSet()
      FileUtils.cleanupDirectory(root, 2)
      assertEquals(retained + "nested", root.listFiles()!!.map { it.name }.toSet())
      assertTrue(File(root, "nested/keep").exists())
      val names =
        planStorageRules(
          StorageRulesRequest(
            operation = StorageOperation.NAMES,
            names = listOf("\uE000", "😀", "\uD800").map { StorageNameInput(it.toUtf16LeBytes()) },
          )
        )
      assertEquals(listOf(2, 1, 0), names.indices)
      val classify =
        planStorageRules(
          StorageRulesRequest(
            operation = StorageOperation.CLASSIFY,
            observations =
              listOf(RecoveryFileObservation(0, false, false, false, ".TAR.GZ".toUtf16LeBytes())),
          )
        )
      assertEquals(listOf(RecoveryCandidateRef(0, RecoveryFileKind.TAR_GZ)), classify.candidates)
    } finally {
      root.deleteRecursively()
    }
  }
}
