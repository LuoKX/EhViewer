package com.hippo.ehviewer.ui.screen

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.core.net.toUri
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.ehviewer.core.i18n.R
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.ui.MainActivity
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Activity-level contract for Gallery Page app-link resolution and navigation.
 *
 * The page-zero URL is intentionally accepted by the frozen navigation grammar and becomes the
 * legacy `pageIndex = -1` sentinel. [ProgressScreen] rejects that sentinel before HTTP, so this
 * test exercises the package-scoped Android intent filter, Activity launch, the intent channel,
 * Rust rollout selection, destination construction, and Compose rendering without contacting a live
 * service. Real JNI reachability is asserted separately by `MvpFlowDeviceTest`; verified-domain
 * default routing still requires a separate OS-level test.
 */
@RunWith(AndroidJUnit4::class)
class GalleryNavigationAppLinkDeviceTest {
  @get:Rule val composeRule = createEmptyComposeRule()

  @Test
  fun packageScopedManifestResolvesSupportedGalleryHosts() {
    val context = ApplicationProvider.getApplicationContext<Context>()

    SUPPORTED_GALLERY_URLS.forEach { url ->
      assertMainActivityResolution(context, galleryViewIntent(context, url))
    }
  }

  @Test
  fun packageScopedManifestRejectsUnsupportedGalleryLinks() {
    val context = ApplicationProvider.getApplicationContext<Context>()

    UNSUPPORTED_GALLERY_URLS.forEach { url ->
      val resolved =
        context.packageManager.resolveActivity(
          galleryViewIntent(context, url),
          PackageManager.MATCH_DEFAULT_ONLY,
        )
      assertNull(resolved)
    }
  }

  @Test
  fun packageScopedViewIntentOpensTheProgressDestinationWithoutHttp() {
    val context = ApplicationProvider.getApplicationContext<Context>()
    val downloadDirectory = checkNotNull(context.getExternalFilesDir(null)).resolve("download")
    val removeDownloadDirectory = !downloadDirectory.isDirectory
    check(downloadDirectory.mkdirs() || downloadDirectory.isDirectory) {
      "Unable to prepare the Activity download directory"
    }
    try {
      val previousSettings = runBlocking { Settings.snapshot() }
      try {
        runBlocking {
          Settings.withMutableSnapshot { snapshot ->
            snapshot[Settings.useRustCore] = true
            snapshot[Settings.needSignIn] = true
            snapshot[Settings.updateIntervalDays] = 0
          }
        }

        val intent = galleryViewIntent(context, PAGE_ZERO_URL)
        assertMainActivityResolution(context, intent)
        ActivityScenario.launch<MainActivity>(intent).use {
          val expectedError = context.getString(R.string.error_something_wrong_happened)
          composeRule.waitUntil(timeoutMillis = DESTINATION_TIMEOUT_MILLIS) {
            composeRule
              .onAllNodesWithTag(PROGRESS_SCREEN_TEST_TAG)
              .fetchSemanticsNodes()
              .isNotEmpty()
          }
          composeRule.waitUntil(timeoutMillis = DESTINATION_TIMEOUT_MILLIS) {
            composeRule.onAllNodesWithText(expectedError).fetchSemanticsNodes().isNotEmpty()
          }

          composeRule.onNodeWithTag(PROGRESS_SCREEN_TEST_TAG).assertIsDisplayed()
          composeRule.onNodeWithText(expectedError).assertIsDisplayed()
        }
      } finally {
        runBlocking {
          Settings.withMutableSnapshot { snapshot ->
            snapshot[Settings.useRustCore] = previousSettings[Settings.useRustCore]
            snapshot[Settings.needSignIn] = previousSettings[Settings.needSignIn]
            snapshot[Settings.updateIntervalDays] = previousSettings[Settings.updateIntervalDays]
          }
        }
      }
    } finally {
      if (removeDownloadDirectory) {
        check(downloadDirectory.delete()) { "Unable to restore the Activity download directory" }
      }
    }
  }

  private fun galleryViewIntent(context: Context, url: String) =
    Intent(Intent.ACTION_VIEW, url.toUri()).apply {
      addCategory(Intent.CATEGORY_BROWSABLE)
      setPackage(context.packageName)
    }

  private fun assertMainActivityResolution(context: Context, intent: Intent) {
    val resolved =
      context.packageManager
        .resolveActivity(intent, PackageManager.MATCH_DEFAULT_ONLY)
        ?.activityInfo
    assertEquals(MainActivity::class.java.name, resolved?.name)
  }

  private companion object {
    const val DESTINATION_TIMEOUT_MILLIS = 15_000L
    const val PAGE_ZERO_URL = "https://e-hentai.org/s/abcdef0123/42-0"
    val SUPPORTED_GALLERY_URLS =
      listOf(
        "http://e-hentai.org/",
        "https://e-hentai.org/",
        "http://exhentai.org/",
        "https://exhentai.org/",
      )
    val UNSUPPORTED_GALLERY_URLS =
      listOf(
        "https://example.org/",
        "ftp://e-hentai.org/",
      )
  }
}
