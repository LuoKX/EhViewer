package com.hippo.ehviewer.download.recovery

import android.net.Uri
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ehviewer.core.database.model.DownloadRecoverySourceKind
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryPlan
import com.hippo.ehviewer.bridge.download.planRecoveryDirectory
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.util.UUID
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real JNI/provider adapter evidence; compiling this source is not device execution. */
@RunWith(AndroidJUnit4::class)
class RecoveryDirectoryDeviceTest {
  @Test
  fun directoryReadsOnlyRustSelectedSidecarsInStableJavaSourceOrder() {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val root =
      File(
          context.cacheDir,
          "directory-plan-${UUID.randomUUID()}",
        )
        .apply { check(mkdir()) }

    fun node(
      id: String,
      name: String,
      size: Long?,
      modified: Long?,
      directory: Boolean = false,
    ) =
      RecoveryDocumentNode(
        "root",
        id,
        "root",
        Uri.fromFile(File(root, name)),
        name,
        null,
        size,
        modified,
        0,
        name,
        1,
        directory,
      )
    try {
      val directory = node("root", "root", null, null, true)
      val children =
        listOf(
          node("\uE000", ".ehviewer", 7, -1),
          node("😀", ".EHVIEWER", 3, -5),
          node("😀", ".ehviewer", 4, -3),
          node("a", "ComicInfo.xml", 2, 0),
          node("b", "empty.jpg", 0, null),
          node("c", "unknown.jpg", null, null),
        )
      children.take(4).forEach { File(root, it.displayName).writeText(it.sourceId) }
      val read = mutableListOf<Int>()
      val readable = mutableListOf<Int>()
      val result =
        observeRecoveryDirectory(
          "root",
          directory,
          children,
          readBounded = {
            read += children.indexOf(it)
            File(root, it.displayName).readBytes()
          },
          canRead = {
            readable += children.indexOf(it)
            true
          },
        )
      val ordered = children.sortedBy { it.sourceId }
      assertEquals(ordered.map { it.displayName }, result.entries.map { it.name })
      assertEquals(listOf(1, 3), read)
      assertEquals(ordered.sumOf { it.size ?: 0L }, result.fingerprint.size)
      assertEquals(
        ordered
          .mapNotNull {
            it.modifiedAtMillis
          }
          .maxOrNull(),
        result.fingerprint.modifiedAtMillis,
      )
      assertFalse(4 in readable)
      assertTrue(5 in readable)
      assertArrayEquals(File(root, ".EHVIEWER").readBytes(), result.spiderInfo)
      assertArrayEquals(File(root, "ComicInfo.xml").readBytes(), result.comicInfo)
    } finally {
      root.deleteRecursively()
    }
  }

  @Test
  fun rejectedWholeDirectoryPlanNeverReadsProviderFiles() {
    val uri = Uri.parse("content://fixture/node")
    val node =
      RecoveryDocumentNode(
        "root",
        "id",
        "root",
        uri,
        ".ehviewer",
        null,
        1,
        null,
        0,
        "dir",
        1,
        false,
      )
    var reads = 0
    try {
      observeRecoveryDirectory(
        "root",
        node,
        listOf(node),
        readBounded = {
          reads++
          byteArrayOf()
        },
        canRead = {
          reads++
          true
        },
        plan = { request ->
          planRecoveryDirectory(request) { _, _, output ->
            val bytes =
              rustCbor.encodeToByteArray(
                ApiEnvelope(
                  1,
                  "recoveryDirectory",
                  payload = RecoveryDirectoryPlan(listOf(99), 0, null, 1, null, emptyList()),
                )
              )
            output.put(bytes)
            bytes.size
          }
        },
      )
      fail("Malformed native plan accepted")
    } catch (_: IllegalArgumentException) {
      assertEquals(0, reads)
    }
  }

  @Test
  fun scanBatchAndEhApplyUseSameRealJniPromotionContract() {
    val sources =
      listOf(
        RecoveryPromotionInput("dir/42.CBZ", DownloadRecoverySourceKind.CBZ, 42),
        RecoveryPromotionInput("dir/42.ZİP", DownloadRecoverySourceKind.ZIP, 42),
        RecoveryPromotionInput("42.zip", DownloadRecoverySourceKind.ZIP, 42),
        RecoveryPromotionInput("dir", DownloadRecoverySourceKind.DIRECTORY, 42),
        RecoveryPromotionInput("dir/42.rar", DownloadRecoverySourceKind.RAR, 42),
        RecoveryPromotionInput("dir", DownloadRecoverySourceKind.DIRECTORY, 0),
      )
    val expected = listOf("dir", "dir", null, "dir", null, null)
    assertEquals(expected, recoveryPromotionPaths(sources))
    assertEquals(expected, sources.map { recoveryPromotionPath(it.relativePath, it.kind, it.gid) })
  }
}
