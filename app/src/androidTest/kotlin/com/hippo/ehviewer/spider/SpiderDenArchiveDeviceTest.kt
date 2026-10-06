package com.hippo.ehviewer.spider

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ehviewer.core.database.model.GalleryEntity
import com.hippo.ehviewer.Settings
import java.io.File
import java.io.IOException
import java.util.Collections
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import kotlinx.coroutines.runBlocking
import okio.Path.Companion.toOkioPath
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Device regression for the final-page transition into generated-CBZ completion. */
@RunWith(AndroidJUnit4::class)
class SpiderDenArchiveDeviceTest {
  @Test
  fun usesOperationPageCountAndKeepsRealArchiveFailuresDistinct() = runBlocking {
    val context = ApplicationProvider.getApplicationContext<android.content.Context>()
    val testRoot = File(context.cacheDir, "spider-den-archive-${System.nanoTime()}")
    check(testRoot.mkdir()) { "Unable to create the SpiderDen archive test directory" }
    val previousSaveAsCbz = Settings.snapshot()[Settings.saveAsCbz]

    try {
      Settings.withMutableSnapshot { snapshot -> snapshot[Settings.saveAsCbz] = true }

      val completeInfo = galleryInfo(gid = 8_888_881L, pages = 1)
      val completeDir = File(testRoot, "complete").apply { check(mkdir()) }
      writePage(completeDir, index = 0)
      writePage(completeDir, index = 1)
      val completeDen = SpiderDen(completeInfo, completeDir.toOkioPath())

      val completeFailed =
        completeDen.archive(expectedPages = 2) {
          throw IOException("injected metadata network failure")
        }

      assertFalse(completeFailed)
      assertEquals(2, completeInfo.pages)
      assertEquals(
        2,
        checkNotNull(readComicInfo(completeDir.resolve(COMIC_INFO_FILE).toOkioPath())).pageCount,
      )
      ZipFile(completeDir.resolve("${completeInfo.gid}.cbz")).use { archive ->
        val entries = Collections.list(archive.entries())
        assertEquals(
          listOf("00000001.jpg", "00000002.jpg", COMIC_INFO_FILE),
          entries.map(ZipEntry::getName),
        )
        entries.forEach { entry -> assertEquals(ZipEntry.STORED, entry.method) }
      }

      val incompleteInfo = galleryInfo(gid = 8_888_882L, pages = 1)
      val incompleteDir = File(testRoot, "incomplete").apply { check(mkdir()) }
      writePage(incompleteDir, index = 0)
      val incompleteDen = SpiderDen(incompleteInfo, incompleteDir.toOkioPath())

      val incompleteFailed = incompleteDen.archive(expectedPages = 2) {}

      assertTrue(incompleteFailed)
      assertFalse(incompleteDir.resolve("${incompleteInfo.gid}.cbz").exists())
    } finally {
      Settings.withMutableSnapshot { snapshot ->
        snapshot[Settings.saveAsCbz] = previousSaveAsCbz
      }
      check(testRoot.deleteRecursively() || !testRoot.exists()) {
        "Unable to remove the SpiderDen archive test directory"
      }
    }
  }

  private fun writePage(directory: File, index: Int) {
    directory.resolve(perFilename(index, "jpg")).writeBytes("page-$index".encodeToByteArray())
  }

  private fun galleryInfo(gid: Long, pages: Int) =
    GalleryEntity(
      gid = gid,
      token = "0123456789",
      title = "Archive completion regression",
      titleJpn = null,
      thumbKey = null,
      category = 0,
      posted = null,
      uploader = null,
      rating = 0f,
      simpleTags = emptyList(),
      pages = pages,
      simpleLanguage = null,
      favoriteSlot = -2,
    )
}
