package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import kotlin.test.*

class EntryActionsBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun stablePartitionAndAllDefaultFlagCombinations() {
    for (bits in 0..255) {
      val facts = BooleanArray(8) { bits and (1 shl it) != 0 }
      assertEquals(facts.indices.partition { facts[it] }, partitionDownloadEntries(facts))
    }
    assertEquals(emptyList<Int>() to emptyList<Int>(), partitionDownloadEntries(booleanArrayOf()))
    for (bits in 0..31) {
      val f = BooleanArray(5) { bits and (1 shl it) != 0 }
      var direct = f[0]
      var used = false
      if (!direct && f[1]) {
        direct = f[2] || f[3]
        used = direct
      }
      if (!direct && f[4]) {
        direct = true
        used = false
      }
      assertEquals(
        DownloadEntryChoice(direct, used),
        downloadEntryChoice(f[0], f[1], f[2], f[3], f[4]),
      )
    }
  }

  @Test
  fun favoriteChoicesUseCapturedRemovalBasisAndPreserveStoredSlots() {
    for (signed in listOf(false, true)) for (missing in listOf(false, true)) for (current in
      listOf(-9, -2, -1, 0, 9, 10)) for (default in
      listOf(Int.MIN_VALUE, -2, -1, 0, 9, Int.MAX_VALUE)) {
      val plan = favoriteEntryChoice(signed, current, default, missing)
      assertEquals(signed && default == -2, plan.chooser)
      assertEquals(plan.chooser && current != -2, plan.removeRow)
      assertEquals(plan.chooser && current >= 0 && missing, plan.fetchNote)
      if (!plan.chooser)
        assertEquals(if (!signed) -1 else if (current != -2) -2 else default, plan.slot)
    }
    for (remove in listOf(false, true)) for (index in 0 until if (remove) 12 else 11) {
      assertEquals(if (remove) index - 2 else index - 1, selectedFavoriteSlot(remove, index))
    }
    val menu = listOf("localized default", "a", "\ud800")
    for (index in menu.indices) assertEquals(
      if (index == 0) null else menu[index],
      selectedDownloadLabel(menu, index),
    )
    assertFails { selectedDownloadLabel(menu, -1) }
    assertFails { selectedDownloadLabel(menu, 3) }
    assertFails { selectedFavoriteSlot(false, 11) }
    assertFails { partitionDownloadEntries(BooleanArray(100_001)) }
  }
}
