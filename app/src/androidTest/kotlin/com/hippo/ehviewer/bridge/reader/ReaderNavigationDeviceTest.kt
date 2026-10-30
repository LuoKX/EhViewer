package com.hippo.ehviewer.bridge.reader

import androidx.compose.ui.geometry.Offset
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.files.prepareDownloadedThumbnail
import eu.kanade.tachiyomi.ui.reader.setting.TappingInvertMode
import eu.kanade.tachiyomi.ui.reader.viewer.ViewerNavigation
import eu.kanade.tachiyomi.ui.reader.viewer.tapIntent
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReaderNavigationDeviceTest {
  @Test
  fun nativeModesAndTapRegionsUseTheDeviceLibrary() {
    assertEquals(4, readerMode(0, listOf("other:webtoon"))[0])
    assertEquals(2, readerMode(-1, listOf("WEBTOON"))[0])
    val regions = ViewerNavigation.fromPreference(4, false).regions(TappingInvertMode.NONE)
    assertArrayEquals(intArrayOf(2, 2), regions.tapIntent(Offset(.1f, .5f), true, false))
    assertArrayEquals(intArrayOf(0, 0), regions.tapIntent(Offset(Float.NaN, .5f), true, false))
  }

  @Test
  fun thumbnailMetadataPrecedesDeviceSuffixValidation() = runBlocking {
    val events = mutableListOf<String>()
    assertEquals(
      "dir" to "thumb.PNG",
      prepareDownloadedThumbnail(
        "old",
        "a.PNG",
        { events += "metadata" },
        {
          events += "directory"
          "dir"
        },
      ),
    )
    assertEquals(listOf("metadata", "directory"), events)
  }
}
