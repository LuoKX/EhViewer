package com.hippo.ehviewer.bridge.gallery

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI coverage for similar-gallery title cleanup. */
@RunWith(AndroidJUnit4::class)
class GalleryTitleDeviceTest {
  @Test
  fun extractsTitlesThroughTheDefaultJniBoundary() {
    assertEquals(
      "Title",
      extractGalleryTitle("[Group] (Event) Title | Translation CHx 1-2 [Digital]"),
    )
    assertNull(extractGalleryTitle("[Only] {metadata} ~here~"))
    assertNull(extractGalleryTitle(null))
  }
}
