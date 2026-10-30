package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class FavoriteBatchBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun crossStoreActionsAndMenusKeepTheirOwnSentinels() {
    for (source in listOf(Int.MIN_VALUE, -3, -2, -1, 0, 5, 9, 10, Int.MAX_VALUE)) {
      assertEquals(
        if (source == -2) FavoriteBatchAction.DELETE_LOCAL else FavoriteBatchAction.DELETE_CLOUD,
        planFavoriteBatch(source).action,
      )
      for (destination in listOf(-2) + (0..9)) {
        val expected =
          when {
            source == destination -> FavoriteBatchAction.NONE
            source == -2 -> FavoriteBatchAction.UPLOAD_THEN_REMOVE_LOCAL
            destination == -2 -> FavoriteBatchAction.SAVE_LOCAL
            else -> FavoriteBatchAction.MOVE_CLOUD
          }
        val plan = planFavoriteBatch(source, destination)
        assertEquals(expected, plan.action)
        assertEquals(source != destination, plan.refresh)
      }
    }
    for (size in 1..11) for (index in 0 until size) assertEquals(
      if (index == 0) -2 else index - 1,
      selectedBulkFavoriteDestination(index, size),
    )
    assertFails { selectedBulkFavoriteDestination(11, 11) }
    assertFails { planFavoriteBatch(0, -1) }
  }

  @Test
  fun effectsAreOrderedAndCloudToLocalRetainsCloudRecords() = runBlocking {
    val cases =
      listOf(
        Triple(-2, null, listOf("removeLocal")),
        Triple(2, null, listOf("removeCloud")),
        Triple(-2, 2, listOf("upload", "removeLocal")),
        Triple(2, -2, listOf("saveLocal")),
        Triple(2, 4, listOf("moveCloud")),
        Triple(2, 2, emptyList()),
      )
    for ((source, destination, expected) in cases) {
      val events = mutableListOf<String>()
      executeFavoriteBatch(
        planFavoriteBatch(source, destination),
        { events += "removeLocal" },
        { events += "removeCloud" },
        { events += "upload" },
        { events += "saveLocal" },
        { events += "moveCloud" },
      )
      assertEquals(expected, events)
    }
  }

  @Test
  fun partialUploadFailureAndCancellationNeverRemoveLocalRows() = runBlocking {
    for (failure in
      listOf(IOException("second upload failed"), CancellationException("cancelled"))) {
      val events = mutableListOf<String>()
      val caught = assertFails {
        executeFavoriteBatch(
          planFavoriteBatch(-2, 3),
          { events += "removeLocal" },
          { fail("cloud deletion") },
          {
            events += "uploaded first"
            throw failure
          },
          { fail("save local") },
          { fail("move cloud") },
        )
      }
      assertSame(failure, caught)
      assertEquals(listOf("uploaded first"), events)
    }
    val failure = IOException("Room write failed")
    assertSame(
      failure,
      assertFails {
        executeFavoriteBatch(
          planFavoriteBatch(2, -2),
          { fail("remove local") },
          { fail("remove cloud") },
          { fail("upload") },
          { throw failure },
          { fail("move cloud") },
        )
      },
    )
  }

  @Test
  fun overlayPreservesShortCircuitQueriesAndAwaitOrdering() = runBlocking {
    val events = mutableListOf<Long>()
    for (slot in listOf(-3, -2, -1, 0, 9)) {
      val info =
        BaseGalleryInfo(gid = slot.toLong(), token = "fixture").apply { favoriteSlot = slot }
      applyLocalFavoriteOverlay(info) {
        events += it
        true
      }
      assertEquals(if (slot == -2) -1 else slot, info.favoriteSlot)
    }
    assertEquals(listOf(-2L), events)
    val info = BaseGalleryInfo(gid = 1, token = "fixture").apply { favoriteSlot = -2 }
    applyLocalFavoriteOverlay(info) {
      info.favoriteSlot = 5
      true
    }
    assertEquals(-1, info.favoriteSlot)
    info.favoriteSlot = -2
    applyLocalFavoriteOverlay(info) {
      info.favoriteSlot = 5
      false
    }
    assertEquals(5, info.favoriteSlot)
    info.favoriteSlot = -2
    assertFailsWith<IOException> {
      applyLocalFavoriteOverlay(info) { throw IOException("lookup failed") }
    }
    assertEquals(-2, info.favoriteSlot)
  }

  @Test
  fun invalidNativeIntentsRejectBeforeAnyEffect() {
    for (bad in
      listOf(
        intArrayOf(),
        intArrayOf(99, 1),
        intArrayOf(0, 1),
        intArrayOf(1, 1),
        intArrayOf(3, 1),
        intArrayOf(4, 1),
      )) {
      assertFails { planFavoriteBatch(2, 4) { _, _, _ -> bad } }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      planFavoriteBatch(-2, 1) { _, _, _ -> throw UnsatisfiedLinkError("fixture") }
    }
  }
}
