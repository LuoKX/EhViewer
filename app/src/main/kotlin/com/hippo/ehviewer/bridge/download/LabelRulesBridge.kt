@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

// ByteString applies to fields, so wrap nullable collection elements explicitly.
@Serializable
internal enum class LabelAction {
  @SerialName("lookup") LOOKUP,
  @SerialName("add") ADD,
  @SerialName("assign") ASSIGN,
  @SerialName("matching") MATCHING,
  @SerialName("deletePositions") DELETE_POSITIONS,
  @SerialName("validateName") VALIDATE_NAME,
  @SerialName("defaultSelection") DEFAULT_SELECTION,
  @SerialName("defaultChoice") DEFAULT_CHOICE,
}

@Serializable
internal data class LabelRulesRequest(
  val version: Int,
  val action: LabelAction,
  val names: List<LabelName>,
  @ByteString val name: ByteArray?,
  val positions: List<Int>,
  val index: Int,
  val blank: Boolean? = null,
  @ByteString val reserved: ByteArray? = null,
  val hasDefault: Boolean? = null,
)

@Serializable internal data class LabelName(@ByteString val value: ByteArray?)

@Serializable
internal data class LabelPlan(val value: Int, val indices: List<Int>, val positions: List<Int>)

internal fun planLabelRules(
  action: LabelAction,
  names: List<String?> = emptyList(),
  name: String? = null,
  positions: List<Int> = emptyList(),
  index: Int = 0,
  blank: Boolean? = null,
  reserved: String? = null,
  hasDefault: Boolean? = null,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeLabelRulesV1,
): LabelPlan {
  require(names.size <= 100_000 && positions.size <= 100_000) { "Label snapshot exceeds limit" }
  val texts = names.asSequence().filterNotNull() + sequenceOf(name, reserved).filterNotNull()
  require(
    texts.all { it.length <= 4096 } && texts.sumOf { it.length.toLong() * 2 } <= 4 * 1024 * 1024
  ) {
    "Label text exceeds limit"
  }
  val request =
    LabelRulesRequest(
      version = 1,
      action = action,
      names = names.map { LabelName(it?.toUtf16LeBytes()) },
      name = name?.toUtf16LeBytes(),
      positions = positions,
      index = index,
      blank = blank,
      reserved = reserved?.toUtf16LeBytes(),
      hasDefault = hasDefault,
    )
  val bytes = rustCbor.encodeToByteArray(request)
  require(bytes.size <= 8 * 1024 * 1024) { "Label request exceeds limit" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<LabelPlan>>(
      bytes,
      "Label rules",
      maxResponseSize = 8 * 1024 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "labelRules") {
    "Invalid label rules envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload).also { plan ->
    val slots = if (action == LabelAction.DELETE_POSITIONS) positions.indices else names.indices
    require(
      plan.indices.all { it in slots } && plan.indices.zipWithNext().all { (a, b) -> a < b }
    ) {
      "Invalid label rule slots"
    }
    require(
      if (action == LabelAction.DELETE_POSITIONS) plan.positions.size == plan.indices.size
      else plan.positions.isEmpty()
    ) {
      "Invalid label positions"
    }
    when (action) {
      LabelAction.LOOKUP ->
        require(
          plan.value == -1 ||
            (plan.value in names.indices && name != null && names[plan.value] == name)
        )
      LabelAction.ADD -> require(plan.value == -1 || (plan.value == names.size && name != null))
      LabelAction.ASSIGN -> require(plan.value in 0..1)
      LabelAction.VALIDATE_NAME -> require(plan.value in 0..3)
      LabelAction.DEFAULT_SELECTION -> require(plan.value in 0..names.size + 1)
      LabelAction.DEFAULT_CHOICE -> require(plan.value in -2 until names.size)
      LabelAction.MATCHING -> require(plan.value == 0 && plan.indices.all { names[it] == name })
      LabelAction.DELETE_POSITIONS ->
        require(
          plan.value == 0 &&
            plan.indices == (index + 1 until positions.size).toList() &&
            plan.indices.indices.all { plan.positions[it] == positions[plan.indices[it]] - 1 }
        )
    }
    if (action != LabelAction.MATCHING && action != LabelAction.DELETE_POSITIONS)
      require(plan.indices.isEmpty())
  }
}

internal fun labelIndex(names: List<String?>, name: String?) =
  planLabelRules(LabelAction.LOOKUP, names, name).value

private external fun nativeLabelRulesV1(request: ByteBuffer, size: Int): ByteArray

internal fun validateLabelName(name: String, reserved: String, names: List<String>) =
  planLabelRules(
      LabelAction.VALIDATE_NAME,
      names,
      name,
      blank = name.isBlank(),
      reserved = reserved,
    )
    .value

internal fun defaultLabelSelection(hasDefault: Boolean, name: String?, names: List<String>) =
  planLabelRules(LabelAction.DEFAULT_SELECTION, names, name, hasDefault = hasDefault).value

internal data class DefaultLabelChoice(val hasDefault: Boolean, val label: String?)

internal fun chooseDefaultLabel(index: Int, names: List<String>): DefaultLabelChoice {
  val plan = planLabelRules(LabelAction.DEFAULT_CHOICE, names, index = index)
  return DefaultLabelChoice(plan.value != -2, plan.value.takeIf { it >= 0 }?.let { names[it] })
}
