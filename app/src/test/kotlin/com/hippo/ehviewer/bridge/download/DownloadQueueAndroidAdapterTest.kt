package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.GalleryEntity
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class DownloadQueueAndroidAdapterTest {
  @Test
  fun mapsEveryAndroidAndRustDownloadStateBidirectionally() {
    val mappings =
      listOf(
        DownloadInfo.STATE_INVALID to DownloadState.INVALID,
        DownloadInfo.STATE_NONE to DownloadState.NONE,
        DownloadInfo.STATE_WAIT to DownloadState.WAITING,
        DownloadInfo.STATE_DOWNLOAD to DownloadState.DOWNLOADING,
        DownloadInfo.STATE_FINISH to DownloadState.FINISHED,
        DownloadInfo.STATE_FAILED to DownloadState.FAILED,
      )

    mappings.forEach { (android, rust) ->
      assertEquals(rust, android.toRustDownloadState())
      assertEquals(android, rust.toAndroidDownloadState())
    }
    assertFailsWith<IllegalArgumentException> { Int.MAX_VALUE.toRustDownloadState() }
  }

  @Test
  fun buildsACompleteSnapshotWithoutChangingEntryOrFifoOrder() {
    val active = downloadInfo(3, DownloadInfo.STATE_DOWNLOAD)
    val firstWaiting = downloadInfo(2, DownloadInfo.STATE_WAIT)
    val secondWaiting = downloadInfo(1, DownloadInfo.STATE_WAIT)

    val snapshot =
      buildDownloadQueueSnapshot(
        entries = listOf(secondWaiting, firstWaiting, active),
        waiting = listOf(firstWaiting, secondWaiting),
        current = active,
      )

    assertEquals(listOf(1L, 2L, 3L), snapshot.entries.map(DownloadEntry::galleryId))
    assertEquals(listOf(2L, 1L), snapshot.waiting)
    assertEquals(3L, snapshot.current)
    assertEquals(DownloadState.DOWNLOADING, snapshot.entries.last().state)
  }

  @Test
  fun recoverySelectsOnlyVolatileStatesOutsideTheLiveTopology() {
    val active = downloadInfo(1, DownloadInfo.STATE_DOWNLOAD)
    val waiting = downloadInfo(2, DownloadInfo.STATE_WAIT)
    val orphanWaiting = downloadInfo(3, DownloadInfo.STATE_WAIT)
    val orphanActive = downloadInfo(4, DownloadInfo.STATE_DOWNLOAD)
    val finished = downloadInfo(5, DownloadInfo.STATE_FINISH)

    val recovery =
      buildDownloadQueueRecoveryEntries(
        entries = listOf(active, waiting, orphanWaiting, orphanActive, finished),
        waiting = listOf(waiting),
        current = active,
      )

    assertEquals(listOf(3L, 4L), recovery.map(DownloadEntry::galleryId))
    assertEquals(
      listOf(DownloadState.WAITING, DownloadState.DOWNLOADING),
      recovery.map { it.state },
    )
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
