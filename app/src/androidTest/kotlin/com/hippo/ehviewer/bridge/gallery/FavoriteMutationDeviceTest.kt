package com.hippo.ehviewer.bridge.gallery

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Offline API-35 contract shape for both single and bulk favorite mutation plans. */
@RunWith(AndroidJUnit4::class)
class FavoriteMutationDeviceTest {
  @Test
  fun validatesSingleAndBulkPlansWithoutHttp() {
    val single =
      FavoriteMutationPlan(
        FavoriteMutationKind.SINGLE,
        "https://e-hentai.org/gallerypopups.php?gid=-1&t=opaque&act=addfav",
        "https://e-hentai.org/gallerypopups.php?gid=-1&t=opaque&act=addfav",
        "https://e-hentai.org",
        listOf(
          FavoriteMutationField("favcat", "favdel"),
          FavoriteMutationField("favnote", ""),
          FavoriteMutationField("apply", "Apply Changes"),
          FavoriteMutationField("update", "1"),
        ),
      )
    val bulk =
      FavoriteMutationPlan(
        FavoriteMutationKind.BULK,
        "https://e-hentai.org/favorites.php",
        "https://e-hentai.org/favorites.php",
        "https://e-hentai.org",
        listOf(FavoriteMutationField("ddact", "delete")),
      )
    val requestSingle =
      FavoriteMutationRequest(
        1,
        GalleryUrlSite.PUBLIC,
        single = FavoriteMutationSingle(-1, "opaque", -1, ""),
        bulk = null,
      )
    val requestBulk =
      FavoriteMutationRequest(
        1,
        GalleryUrlSite.PUBLIC,
        single = null,
        bulk = FavoriteMutationBulk(emptyList(), -3, -1),
      )
    assertEquals(single, planFavoriteMutation(requestSingle))
    assertEquals(bulk, planFavoriteMutation(requestBulk))
  }
}
