package com.hippo.ehviewer.bridge.rollout

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.auth.SessionGallerySite
import com.hippo.ehviewer.bridge.auth.SessionLifecycleCommand
import com.hippo.ehviewer.bridge.auth.SessionLifecycleMode
import com.hippo.ehviewer.bridge.auth.SessionLifecycleSnapshot
import com.hippo.ehviewer.bridge.auth.planSessionLifecycle
import com.hippo.ehviewer.bridge.download.DownloadEntry
import com.hippo.ehviewer.bridge.download.DownloadQueueCommand
import com.hippo.ehviewer.bridge.download.DownloadQueuePlan
import com.hippo.ehviewer.bridge.download.DownloadQueueSnapshot
import com.hippo.ehviewer.bridge.download.DownloadState
import com.hippo.ehviewer.bridge.download.DownloadStateChange
import com.hippo.ehviewer.bridge.download.reduceDownloadQueue
import com.hippo.ehviewer.client.parser.GalleryDetailParser
import com.hippo.ehviewer.client.parser.GalleryDetailUrlParser
import com.hippo.ehviewer.client.parser.GalleryListParser
import com.hippo.ehviewer.client.parser.GalleryPageParser
import com.hippo.ehviewer.client.parser.GalleryPageUrlParser
import java.nio.ByteBuffer
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Device contract for the coordinated Rust MVP boundary.
 *
 * This deliberately avoids network and UI claims: it proves that one atomic rollout snapshot can
 * drive the real session, list, detail, navigation, reader, and download JNI/CBOR boundaries with
 * shared offline fixtures. It then commits a global rollback and proves that the next Gallery Page
 * operation selects legacy without invoking Rust. Full startup, HTTP, Compose rendering, active
 * worker, and physical-device rollback E2E remain separate acceptance gates.
 */
@RunWith(AndroidJUnit4::class)
class MvpFlowDeviceTest {
  @Test
  fun runsTheCoordinatedMvpContractsThroughRealJni() = runBlocking {
    val previousSettings = Settings.snapshot()
    try {
      assertEquals("use_rust_core", Settings.useRustCore.key.name)
      Settings.withMutableSnapshot { snapshot -> snapshot[Settings.useRustCore] = true }
      assertTrue(Settings.snapshot()[Settings.useRustCore])

      val session =
        planSessionLifecycle(
          snapshot =
            SessionLifecycleSnapshot(
              hasAuthCookies = true,
              storedSignedIn = true,
              signInRequired = false,
              username = "reader",
              site = SessionGallerySite.EX_HENTAI,
            ),
          command = SessionLifecycleCommand.restore(),
        )
      assertEquals(SessionLifecycleMode.AUTHENTICATED, session.state.mode)

      val galleryList = GalleryListParser.parse(assetBuffer("gallery_list.html"))
      assertEquals("10-2", galleryList.prev)
      assertEquals("20", galleryList.next)
      assertEquals(listOf(123_456L, 654_321L), galleryList.galleryInfoList.map { it.gid })

      val galleryDetail = GalleryDetailParser.parse(assetBuffer("gallery_detail.html")).detail
      assertEquals(42L, galleryDetail.gid)
      assertEquals(12, galleryDetail.pages)
      assertEquals(2, galleryDetail.previewList.size)

      val detailNavigation = checkNotNull(GalleryDetailUrlParser.parse(DETAIL_URL))
      assertEquals(42L, detailNavigation.gid)
      assertEquals("0123456789", detailNavigation.token)

      val pageNavigation = checkNotNull(GalleryPageUrlParser.parse(PAGE_URL))
      assertEquals(42L, pageNavigation.gid)
      assertEquals("abcdef0123", pageNavigation.pToken)
      assertEquals(2, pageNavigation.page)

      val readerPage = GalleryPageParser.parseV1(assetBuffer("reader_page.html"))
      assertEquals("https://img.example/page.jpg?first=1&second=2", readerPage.imageUrl)
      assertEquals("skip-key", readerPage.skipHathKey)
      assertEquals("show123", readerPage.showKey)

      val firstGalleryId = galleryList.galleryInfoList.first().gid
      val secondGalleryId = galleryList.galleryInfoList.last().gid
      var queue =
        DownloadQueueSnapshot(
          entries =
            listOf(
              DownloadEntry(firstGalleryId, DownloadState.NONE),
              DownloadEntry(secondGalleryId, DownloadState.NONE),
            ),
          waiting = emptyList(),
        )

      val firstStarted = reduceDownloadQueue(queue, DownloadQueueCommand.startOne(firstGalleryId))
      assertEquals(
        DownloadQueuePlan(
          snapshot =
            DownloadQueueSnapshot(
              entries =
                listOf(
                  DownloadEntry(firstGalleryId, DownloadState.DOWNLOADING),
                  DownloadEntry(secondGalleryId, DownloadState.NONE),
                ),
              waiting = emptyList(),
              current = firstGalleryId,
            ),
          changes =
            listOf(
              DownloadStateChange(
                firstGalleryId,
                DownloadState.NONE,
                DownloadState.DOWNLOADING,
              )
            ),
          startGalleryId = firstGalleryId,
        ),
        firstStarted,
      )
      queue = firstStarted.snapshot

      val secondQueued = reduceDownloadQueue(queue, DownloadQueueCommand.startOne(secondGalleryId))
      assertEquals(listOf(secondGalleryId), secondQueued.snapshot.waiting)
      assertEquals(firstGalleryId, secondQueued.snapshot.current)
      queue = secondQueued.snapshot

      val firstFinished =
        reduceDownloadQueue(queue, DownloadQueueCommand.finish(firstGalleryId, failed = false))
      assertEquals(secondGalleryId, firstFinished.snapshot.current)
      assertEquals(secondGalleryId, firstFinished.startGalleryId)
      assertEquals(
        listOf(DownloadState.FINISHED, DownloadState.DOWNLOADING),
        firstFinished.snapshot.entries.map { it.state },
      )

      val secondFailed =
        reduceDownloadQueue(
          firstFinished.snapshot,
          DownloadQueueCommand.finish(secondGalleryId, failed = true),
        )
      assertNull(secondFailed.snapshot.current)
      assertEquals(
        listOf(DownloadState.FINISHED, DownloadState.FAILED),
        secondFailed.snapshot.entries.map { it.state },
      )

      Settings.withMutableSnapshot { snapshot -> snapshot[Settings.useRustCore] = false }
      val disabledSettings = Settings.snapshot()
      assertFalse(disabledSettings[Settings.useRustCore])
      var rustNavigationCalled = false
      val rolledBackPageNavigation =
        GalleryPageUrlParser.parseWithRustAdapter(
          url = PAGE_URL,
          strict = true,
          rustEnabled = Settings.useRustCore.value,
          rustParser = { _ ->
            rustNavigationCalled = true
            throw AssertionError("Rust Gallery Navigation ran after the MVP rollback")
          },
        )
      assertFalse(rustNavigationCalled)
      assertEquals(GalleryPageUrlParser.Result(42L, "abcdef0123", 2), rolledBackPageNavigation)
    } finally {
      Settings.withMutableSnapshot { snapshot ->
        snapshot[Settings.useRustCore] = previousSettings[Settings.useRustCore]
      }
    }
  }

  private fun assetBuffer(name: String): ByteBuffer {
    val bytes =
      InstrumentationRegistry.getInstrumentation().context.assets.open(name).use { it.readBytes() }
    return ByteBuffer.allocateDirect(maxOf(bytes.size * 2, MINIMUM_BUFFER_CAPACITY)).apply {
      put(bytes)
      flip()
    }
  }

  private companion object {
    const val MINIMUM_BUFFER_CAPACITY = 64 * 1024
    const val DETAIL_URL = "https://e-hentai.org/g/42/0123456789"
    const val PAGE_URL = "https://e-hentai.org/s/abcdef0123/42-3"
  }
}
