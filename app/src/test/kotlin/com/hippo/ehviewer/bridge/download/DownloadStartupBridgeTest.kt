package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.GalleryEntity
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import kotlin.test.*

class DownloadStartupBridgeTest {
  @Test
  fun rawQueryProjectionThroughRealJniPreservesRowsAndUnknownStates() {
    UpdaterHostLibrary.load()
    val states = listOf(Int.MIN_VALUE, -1, 0, 1, 2, 3, 4, 99, Int.MAX_VALUE)
    val rows = states.mapIndexed { i, state -> downloadInfo(i.toLong() - 5, state) }
    val result = projectDownloadStartupStates(rows)
    assertSame(rows, result)
    assertEquals(
      listOf(Int.MIN_VALUE, -1, 0, 0, 0, 3, 4, 99, Int.MAX_VALUE),
      result.map { it.state },
    )
    assertEquals((0..8).map { it.toLong() - 5 }, result.map { it.gid })
    assertSame(rows, projectDownloadStartupStates(rows))
  }

  @Test
  fun validatesCompleteResultBeforeAnyRowMutationAndDoesNotFallback() {
    for (bad in listOf(intArrayOf(0), intArrayOf(0, 9), intArrayOf(0, 0, 0))) {
      val rows = listOf(downloadInfo(1, 1), downloadInfo(1, 3))
      assertFailsWith<IllegalArgumentException> {
        projectDownloadStartupStates(rows) { raw ->
          assertContentEquals(intArrayOf(1, 3), raw)
          raw[0] = 55
          bad
        }
      }
      assertEquals(listOf(1, 3), rows.map { it.state })
    }
    val rows = listOf(downloadInfo(1, 2))
    assertFailsWith<UnsatisfiedLinkError> {
      projectDownloadStartupStates(rows) { throw UnsatisfiedLinkError("fixture") }
    }
    assertEquals(2, rows.single().state)
    assertTrue(projectDownloadStartupStates(emptyList()) { it }.isEmpty())
  }

  private fun downloadInfo(galleryId: Long, state: Int) =
    DownloadInfo(
        galleryInfo =
          GalleryEntity(
            gid = galleryId,
            token = "token",
            title = null,
            titleJpn = null,
            thumbKey = null,
            category = 0,
            posted = null,
            uploader = null,
            rating = 0f,
            simpleTags = null,
            pages = 0,
            simpleLanguage = null,
            favoriteSlot = -1,
          ),
        dirname = null,
      )
      .apply { this.state = state }
}
