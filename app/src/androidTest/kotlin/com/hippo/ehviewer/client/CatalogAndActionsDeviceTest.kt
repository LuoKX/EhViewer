package com.hippo.ehviewer.client

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.download.LabelAction
import com.hippo.ehviewer.bridge.download.favoriteEntryChoice
import com.hippo.ehviewer.bridge.download.labelIndex
import com.hippo.ehviewer.bridge.download.partitionDownloadEntries
import com.hippo.ehviewer.bridge.download.planLabelRules
import com.hippo.ehviewer.bridge.download.selectedDownloadLabel
import com.hippo.ehviewer.bridge.download.selectedFavoriteSlot
import com.hippo.ehviewer.bridge.gallery.CatalogNumbers
import com.hippo.ehviewer.bridge.gallery.savedSearchProgress
import com.hippo.ehviewer.client.data.ListUrlBuilder
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CatalogAndActionsDeviceTest {
  @Test
  fun navigationAndActionsUsePackagedLibrary() {
    val builder = ListUrlBuilder()
    builder.setIndex("\ud800", false)
    builder.page = 2
    assertEquals(2, builder.page)
    assertEquals("\ud800", builder.toGalleryListUrlState().previous)
    builder.setSeek("2026-10-29")
    assertEquals("2", builder.toGalleryListUrlState().next)
    builder.setRange(0)
    assertNull(builder.toGalleryListUrlState().seek)
    assertEquals("\ud800", savedSearchProgress("name@\ud800"))
    assertEquals(null to 2, CatalogNumbers.neighbors(1))
    assertEquals(2, CatalogNumbers.pageRangeError(21, 40))
    assertEquals(
      listOf(1, 3) to listOf(0, 2),
      partitionDownloadEntries(booleanArrayOf(false, true, false, true)),
    )
    val choice = favoriteEntryChoice(true, 2, -2, true)
    assertTrue(choice.chooser && choice.removeRow && choice.fetchNote)
    assertEquals(-2, selectedFavoriteSlot(choice.removeRow, 0))
    assertNull(selectedDownloadLabel(listOf("default", "a"), 0))
    assertEquals(1, labelIndex(listOf("x", "\ud800", "\udc00"), "\ud800"))
    val plan =
      planLabelRules(
        LabelAction.DELETE_POSITIONS,
        positions = listOf(9, 9, Int.MIN_VALUE),
        index = 0,
      )
    assertEquals(listOf(1, 2), plan.indices)
    assertEquals(listOf(8, Int.MAX_VALUE), plan.positions)
  }

  @Test
  fun rawDownloadStatesUsePackagedPrimitiveBoundary() {
    val method =
      Class.forName("com.hippo.ehviewer.bridge.download.DownloadStartupBridgeKt")
        .getDeclaredMethod("nativeNormalizeDownloadStatesV1", IntArray::class.java)
        .apply { isAccessible = true }
    val states = intArrayOf(-1, 0, 1, 2, 3, 4, Int.MIN_VALUE)
    assertArrayEquals(
      intArrayOf(-1, 0, 0, 0, 3, 4, Int.MIN_VALUE),
      method.invoke(null, states) as IntArray,
    )
    assertArrayEquals(intArrayOf(-1, 0, 1, 2, 3, 4, Int.MIN_VALUE), states)
  }
}
