package com.hippo.ehviewer.client

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.download.chooseDefaultLabel
import com.hippo.ehviewer.bridge.download.defaultLabelSelection
import com.hippo.ehviewer.bridge.download.validateLabelName
import com.hippo.ehviewer.bridge.gallery.checkQuickSearchCreation
import com.hippo.ehviewer.bridge.gallery.createQuickSearchName
import com.hippo.ehviewer.bridge.gallery.executeFavoriteBatch
import com.hippo.ehviewer.bridge.gallery.planFavoriteBatch
import com.hippo.ehviewer.bridge.library.finishOrderedList
import com.hippo.ehviewer.bridge.library.moveOrderedList
import com.hippo.ehviewer.client.data.FavListUrlBuilder
import com.hippo.ehviewer.client.data.ListUrlBuilder
import java.io.IOException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CatalogEditDeviceTest {
  @Test
  fun deviceUnicodeClassificationsAndNavigationUsePackagedJni() {
    val builder = ListUrlBuilder()
    for (name in listOf("a@٤٢", "a@４２", "\ud800@+42")) assertEquals(
      0,
      checkQuickSearchCreation(
          builder.quickSearchSnapshot(),
          listOf(builder.toQuickSearch(name)),
          41,
        )
        .duplicate,
    )
    val name = createQuickSearchName("\u2003X\ud800 ", true, 42, emptyList())
    assertEquals("X\ud800@42", name.createdName())
    val original = FavListUrlBuilder(3, "query", "old", "p", "n")
    assertEquals(original.copy(jumpTo = "date", prev = null, next = "2"), original.seek("date"))
    assertEquals(original.copy(jumpTo = null, prev = "1-0", next = null), original.lastPage())
  }

  @Test
  fun listPositionsAndLabelMenusUseTheirOwnIndices() {
    val rows = mutableListOf(7, 8, 9)
    moveOrderedList(rows, 2, 4, 2)
    assertEquals(listOf(8, 9, 7), rows)
    val updates = mutableListOf<Pair<Int, Int>>()
    finishOrderedList(rows, 0, 2) { row, position -> updates += row to position }
    assertEquals(listOf(8 to 0, 9 to 1, 7 to 2), updates)
    assertEquals(1, defaultLabelSelection(true, "missing", listOf("a")))
    assertEquals("a", chooseDefaultLabel(2, listOf("a")).label)
    assertEquals(2, validateLabelName("default", "default", listOf("default")))
  }

  @Test
  fun failedCloudUploadDoesNotRemoveLocalData() = runBlocking {
    val events = mutableListOf<String>()
    try {
      executeFavoriteBatch(
        planFavoriteBatch(-2, 2),
        { events += "remove" },
        { fail("delete cloud") },
        {
          events += "upload"
          throw IOException("fixture")
        },
        { fail("save") },
        { fail("move") },
      )
      fail("Failure swallowed")
    } catch (_: IOException) {
      assertEquals(listOf("upload"), events)
    }
  }
}
