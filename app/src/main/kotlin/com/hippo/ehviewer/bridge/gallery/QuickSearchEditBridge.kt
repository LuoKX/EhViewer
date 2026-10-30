@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.database.model.QuickSearch
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.QuickSearchSnapshot
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

@Serializable internal data class QuickSearchDigit(val unit: Int, val value: Int)

@Serializable
internal data class QuickSearchSaved(
  val query: QuickSearchSnapshot,
  @ByteString val name: ByteArray,
)

@Serializable internal data class QuickSearchName(@ByteString val value: ByteArray)

@Serializable
internal enum class QuickSearchEditKind {
  @SerialName("check") CHECK,
  @SerialName("create") CREATE,
}

@Serializable
internal enum class QuickSearchNameError {
  @SerialName("empty") EMPTY,
  @SerialName("duplicate") DUPLICATE,
}

@Serializable
internal data class QuickSearchEditRequest(
  val version: Int,
  val kind: QuickSearchEditKind,
  val current: QuickSearchSnapshot? = null,
  val saved: List<QuickSearchSaved>,
  val firstGid: Long?,
  val digits: List<QuickSearchDigit>,
  @ByteString val input: ByteArray?,
  val whitespace: List<Int>,
  val saveProgress: Boolean?,
  val next: Long?,
  val names: List<QuickSearchName>,
)

@Serializable
internal data class QuickSearchEditPlan(
  val kind: QuickSearchEditKind,
  val next: Long?,
  val duplicate: Int?,
  @ByteString val name: ByteArray?,
  val position: Int?,
  val error: QuickSearchNameError?,
) {
  fun createdName(): String =
    checkNotNull(name).let { bytes ->
      CharArray(bytes.size / 2) {
          ((bytes[it * 2].toInt() and 255) or ((bytes[it * 2 + 1].toInt() and 255) shl 8)).toChar()
        }
        .concatToString()
    }
}

/** Classifications come from Android/JVM; Rust owns suffix grammar and signed overflow checks. */
internal fun checkQuickSearchCreation(
  current: QuickSearchSnapshot,
  saved: List<QuickSearch>,
  firstGid: Long,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeQuickSearchEditV1,
): QuickSearchEditPlan {
  require(saved.size <= 100_000 && saved.all { it.name.length <= 16_384 }) {
    "Quick search snapshot exceeds limit"
  }
  val digits = linkedMapOf<Int, Int>()
  saved.forEach { row ->
    row.name.forEach { unit ->
      if (unit.code >= 128)
        Character.digit(unit, 10).takeIf { it >= 0 }?.let { digits[unit.code] = it }
    }
  }
  return planQuickSearchEdit(
    QuickSearchEditRequest(
      1,
      QuickSearchEditKind.CHECK,
      current,
      saved.map { QuickSearchSaved(QuickSearchSnapshot.from(it), it.name.toUtf16LeBytes()) },
      firstGid,
      digits.map { QuickSearchDigit(it.key, it.value) },
      null,
      emptyList(),
      null,
      null,
      emptyList(),
    ),
    nativeCall,
  )
}

internal fun createQuickSearchName(
  input: String,
  saveProgress: Boolean,
  next: Long,
  names: List<String>,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeQuickSearchEditV1,
): QuickSearchEditPlan {
  require(input.length <= 16_384 && names.size <= 100_000 && names.all { it.length <= 16_384 }) {
    "Quick search names exceed limit"
  }
  return planQuickSearchEdit(
    QuickSearchEditRequest(
      1,
      QuickSearchEditKind.CREATE,
      null,
      emptyList(),
      null,
      emptyList(),
      input.toUtf16LeBytes(),
      input.asSequence().filter { it.isWhitespace() }.map { it.code }.distinct().toList(),
      saveProgress,
      next,
      names.map { QuickSearchName(it.toUtf16LeBytes()) },
    ),
    nativeCall,
  )
}

internal fun planQuickSearchEdit(
  request: QuickSearchEditRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeQuickSearchEditV1,
): QuickSearchEditPlan {
  val bytes = rustCbor.encodeToByteArray(request)
  require(bytes.size <= 16 * 1024 * 1024) { "Quick search request exceeds limit" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<QuickSearchEditPlan>>(
      bytes,
      "Quick search edit",
      maxResponseSize = 64 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "quickSearchEdit") {
    "Invalid quick search envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload).also { plan ->
    require(plan.kind == request.kind) { "Quick search phase mismatch" }
    when (request.kind) {
      QuickSearchEditKind.CHECK ->
        require(
          plan.next != null &&
            (plan.duplicate == null || plan.duplicate in request.saved.indices) &&
            plan.name == null &&
            plan.position == null &&
            plan.error == null
        )
      QuickSearchEditKind.CREATE -> {
        require(plan.next == null && plan.duplicate == null)
        if (plan.error == null)
          require(
            plan.name != null &&
              plan.name.size in 2..32 * 1024 &&
              plan.name.size % 2 == 0 &&
              plan.position == request.names.size
          )
        else require(plan.name == null && plan.position == null)
      }
    }
  }
}

private external fun nativeQuickSearchEditV1(request: ByteBuffer, size: Int): ByteArray
