package com.hippo.ehviewer.bridge.library

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class ListOrderAction {
  @SerialName("move") MOVE,
  @SerialName("finish") FINISH,
  @SerialName("delete") DELETE,
}

@Serializable
internal data class ListOrderRequest(
  val version: Int,
  val action: ListOrderAction,
  val count: Int,
  val from: Int,
  val to: Int,
  val headers: Int,
  val positions: List<Int>,
)

@Serializable
internal data class ListOrderPlan(
  val from: Int?,
  val to: Int?,
  val indices: List<Int>,
  val positions: List<Int>,
)

internal fun planListOrder(
  action: ListOrderAction,
  count: Int,
  from: Int,
  to: Int = 0,
  headers: Int = 0,
  positions: List<Int> = emptyList(),
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeListOrderV1,
): ListOrderPlan {
  require(count in 0..100_000 && positions.size <= 100_000) { "List snapshot exceeds limit" }
  val request = ListOrderRequest(1, action, count, from, to, headers, positions)
  val bytes = rustCbor.encodeToByteArray(request)
  require(bytes.size <= 2 * 1024 * 1024) { "List order request exceeds limit" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<ListOrderPlan>>(
      bytes,
      "List order",
      maxResponseSize = 2 * 1024 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "listOrder") { "Invalid list order envelope" }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload).also { plan ->
    require(
      plan.indices.size == plan.positions.size &&
        plan.indices.all { it in 0 until count } &&
        plan.indices.zipWithNext().all { (a, b) -> a < b }
    ) {
      "Invalid list order positions"
    }
    if (action == ListOrderAction.MOVE)
      require(
        plan.from != null &&
          plan.to != null &&
          plan.from in 0 until count &&
          plan.to in 0 until count &&
          plan.from.toLong() == from.toLong() - headers &&
          plan.to.toLong() == to.toLong() - headers &&
          plan.indices.isEmpty()
      )
    else {
      require(plan.from == null && plan.to == null)
      when (action) {
        ListOrderAction.FINISH -> {
          val expected =
            if (from == -1 || from == to) emptyList()
            else {
              require(from in 0 until count && to in 0 until count)
              (minOf(from, to)..maxOf(from, to)).toList()
            }
          require(plan.indices == expected && plan.positions == expected) {
            "Incomplete drag completion plan"
          }
        }
        ListOrderAction.DELETE ->
          require(
            positions.size == count &&
              from in positions.indices &&
              plan.indices == (from + 1 until count).toList() &&
              plan.indices.indices.all { plan.positions[it] == positions[plan.indices[it]] - 1 }
          ) {
            "Invalid deletion suffix plan"
          }
        ListOrderAction.MOVE -> error("Unexpected move")
      }
    }
  }
}

/** Complete planning precedes the one remove/insert effect. */
internal fun <T> moveOrderedList(rows: MutableList<T>, from: Int, to: Int, headers: Int = 0) {
  val plan = planListOrder(ListOrderAction.MOVE, rows.size, from, to, headers)
  rows.add(checkNotNull(plan.to), rows.removeAt(checkNotNull(plan.from)))
}

/** Returns exactly the rows whose position was changed, in current list order. */
internal fun <T> finishOrderedList(
  rows: List<T>,
  from: Int,
  to: Int,
  assign: (T, Int) -> Unit,
): List<T> {
  val plan = planListOrder(ListOrderAction.FINISH, rows.size, from, to)
  val selected = plan.indices.map { rows[it] }
  selected.forEachIndexed { i, row -> assign(row, plan.positions[i]) }
  return selected
}

private external fun nativeListOrderV1(request: ByteBuffer, size: Int): ByteArray
