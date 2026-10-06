package com.hippo.ehviewer.bridge.reader

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.spider.SpiderInfo
import com.hippo.ehviewer.spider.applyPreviews
import com.hippo.ehviewer.spider.previewIndex
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.BeforeClass
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real JNI contract; HTTP and worker lifecycle stay Android-owned. */
@RunWith(AndroidJUnit4::class)
class PreviewTokenDeviceTest {
  @Test
  fun lookupAndInitialAndLaterPreviewsUseProductionDelegates() {
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(99 to "retained"))
    info.applyPreviews(listOf(PreviewToken(0, "first"), PreviewToken(0, "")), 0)
    assertEquals(2, info.previewPerPage)
    assertEquals(3, info.previewIndex(7))
    info.applyPreviews(
      listOf(PreviewToken(40, "old"), PreviewToken(40, ""), PreviewToken(40, "new")),
      2,
    )
    assertEquals(20, info.previewPerPage)
    assertEquals(2, info.previewIndex(41))
    assertEquals(mapOf(0 to "first", 40 to "new", 99 to "retained"), info.pTokenMap)
  }

  @Test
  fun emptyNonzeroFailsBeforeMutationAndSignedOverflowRemainsJvmCompatible() {
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(0 to "retained"), previewPerPage = 20)
    assertThrows(RustFailureException::class.java) { info.applyPreviews(emptyList(), 1) }
    assertEquals(20, info.previewPerPage)
    assertEquals(mapOf(0 to "retained"), info.pTokenMap)
    info.applyPreviews(listOf(PreviewToken(Int.MIN_VALUE, "opaque")), -1)
    assertEquals(Int.MIN_VALUE, info.previewPerPage)
    assertEquals(0, info.previewIndex(Int.MAX_VALUE))
    info.applyPreviews(emptyList(), 0)
    assertEquals(0, info.previewPerPage)
  }

  companion object {
    @BeforeClass
    @JvmStatic
    fun loadLibrary() {
      System.loadLibrary("ehviewer")
    }
  }
}
