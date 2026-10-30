package com.hippo.ehviewer.bridge.library

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class ListOrderBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  private data class Row(val id: Int, var position: Int)

  @Test
  fun moveAndFinishMatchOriginalOperationsWithBothHeaderCounts() {
    for (count in 1..8) for (from in 0 until count) for (to in 0 until count) for (headers in
      listOf(0, 2)) {
      val expected = MutableList(count) { Row(it, 100 + it * 10) }
      val rows = expected.map { it.copy() }.toMutableList()
      expected.add(to, expected.removeAt(from))
      val changed =
        if (from == to) emptyList()
        else
          expected.slice(minOf(from, to)..maxOf(from, to)).also { slice ->
            slice.forEachIndexed { i, row -> row.position = minOf(from, to) + i }
          }
      moveOrderedList(rows, from + headers, to + headers, headers)
      val actualChanged =
        finishOrderedList(rows, from, to) { row, position -> row.position = position }
      assertEquals(expected, rows)
      assertEquals(changed, actualChanged)
    }
  }

  @Test
  fun inactiveDragsAndDeletionRespectOriginalSparsePositions() {
    val rows = listOf(Row(1, 7), Row(2, 7), Row(3, Int.MIN_VALUE), Row(4, 99))
    assertTrue(
      finishOrderedList(rows, -1, 9) { _, _ -> fail("Inactive drag mutated rows") }.isEmpty()
    )
    assertTrue(finishOrderedList(rows, 2, 2) { _, _ -> fail("Same slot reindexed rows") }.isEmpty())
    val plan =
      planListOrder(ListOrderAction.DELETE, rows.size, 1, positions = rows.map { it.position })
    assertEquals(listOf(2, 3), plan.indices)
    assertEquals(listOf(Int.MAX_VALUE, 98), plan.positions)
  }

  @Test
  fun invalidMovesAndIncompletePlansRejectBeforeMutation() {
    val rows = mutableListOf(1, 2, 3)
    assertFails { moveOrderedList(rows, 0, 2, 2) }
    assertEquals(listOf(1, 2, 3), rows)
    for (plan in
      listOf(
        ListOrderPlan(null, null, listOf(0), listOf(0)),
        ListOrderPlan(null, null, listOf(0, 2), listOf(0, 2)),
        ListOrderPlan(null, null, listOf(0, 1, 2), listOf(0, 9, 2)),
        ListOrderPlan(null, null, listOf(0, 0), listOf(0, 0)),
      )) {
      assertFails {
        planListOrder(ListOrderAction.FINISH, 3, 0, 2) { _, _ ->
          rustCbor.encodeToByteArray(ApiEnvelope(1, "listOrder", plan))
        }
      }
    }
    assertFails {
      planListOrder(ListOrderAction.MOVE, 3, 0, 2) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(1, "listOrder", ListOrderPlan(1, 2, emptyList(), emptyList()))
        )
      }
    }
    assertFails {
      planListOrder(ListOrderAction.DELETE, 3, 0, positions = listOf(9, 9, 9)) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(1, "listOrder", ListOrderPlan(null, null, listOf(1, 2), listOf(0, 1)))
        )
      }
    }
    assertFails { planListOrder(ListOrderAction.MOVE, 3, 0, 2) { _, _ -> byteArrayOf() } }
    assertFailsWith<UnsatisfiedLinkError> {
      planListOrder(ListOrderAction.MOVE, 3, 0, 2) { _, _ -> throw UnsatisfiedLinkError("fixture") }
    }
  }
}
