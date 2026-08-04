package com.hippo.ehviewer.bridge.gallery

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI coverage for list and H@H download planning; no HTTP is performed. */
@RunWith(AndroidJUnit4::class)
class GalleryArchiveRequestDeviceTest {
  @Test
  fun plansListAndDownloadThroughTheDefaultJniBoundary() {
    val listRequest =
      GalleryArchiveRequest(
        version = 1,
        site = GalleryUrlSite.EX_HENTAI,
        kind = GalleryArchiveRequestKind.LIST,
        gid = -7,
        token = "a b?",
      )
    val listPlan =
      GalleryArchiveRequestPlan(
        kind = GalleryArchiveRequestKind.LIST,
        url = "https://exhentai.org/archiver.php?gid=-7&token=a+b%3F",
        referer = "https://exhentai.org/g/-7/a%20b%3F",
        origin = null,
        form = emptyList(),
      )
    val downloadRequest =
      GalleryArchiveRequest(
        version = 1,
        site = GalleryUrlSite.EX_HENTAI,
        kind = GalleryArchiveRequestKind.DOWNLOAD,
        gid = -7,
        token = "a b?",
        resolution = "1280 weird/?",
        isHath = true,
      )
    val downloadPlan =
      GalleryArchiveRequestPlan(
        kind = GalleryArchiveRequestKind.DOWNLOAD,
        url = "https://exhentai.org/archiver.php?gid=-7&token=a+b%3F",
        referer = "https://exhentai.org/g/-7/a%20b%3F",
        origin = "https://exhentai.org",
        form = listOf(GalleryFormField("hathdl_xres", "1280 weird/?")),
      )

    assertEquals(listPlan, planGalleryArchiveRequest(listRequest))
    assertEquals(downloadPlan, planGalleryArchiveRequest(downloadRequest))
  }
}
