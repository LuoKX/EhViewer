@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.text

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class BBCodeOperation {
  @SerialName("export") EXPORT,
  @SerialName("edit") EDIT,
}

@Serializable
internal enum class BBCodeAction {
  @SerialName("export") EXPORT,
  @SerialName("original") ORIGINAL,
  @SerialName("target") TARGET,
  @SerialName("splice") SPLICE,
}

/** Raw Java UTF-16 units preserve surrogate boundaries, including lone surrogates. */
@Serializable
internal data class BBCodeRequest(
  @EncodeDefault val version: Int = 1,
  val operation: BBCodeOperation,
  val units: List<Int>,
  val originUnits: List<Int>?,
  val spans: List<TextSpan>,
)

@Serializable
internal data class BBCodeEdit(
  val sourceStart: Int,
  val sourceEnd: Int,
  val targetStart: Int,
  val targetEnd: Int,
)

@Serializable
internal data class BBCodePlan(
  val action: BBCodeAction,
  val units: List<Int>?,
  val edit: BBCodeEdit?,
)

/**
 * Rust owns decisions; Kotlin validates transport and applies slices to opaque Compose annotations.
 */
internal fun planBBCode(
  request: BBCodeRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanBBCodeV1,
): BBCodePlan {
  require(request.version == 1 && validUnits(request.units) && request.spans.size <= 100_000) {
    "Invalid BBCode input"
  }
  require(request.originUnits?.let(::validUnits) != false) { "Invalid BBCode input" }
  request.spans.forEach {
    require(
      it.start >= 0 && it.start <= it.end && it.end <= request.units.size && it.flags in 0..15
    ) {
      "Invalid BBCode input"
    }
  }
  require(
    when (request.operation) {
      BBCodeOperation.EXPORT -> request.originUnits == null
      BBCodeOperation.EDIT -> request.originUnits != null && request.spans.isEmpty()
    }
  ) {
    "Invalid BBCode operation fields"
  }
  val envelope =
    invokeRustCbor<ApiEnvelope<BBCodePlan>>(
      rustCbor.encodeToByteArray(request),
      "BBCode",
      nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "bbcode") { "Invalid BBCode envelope" }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing BBCode payload" }
  when (request.operation) {
    BBCodeOperation.EXPORT -> {
      require(plan.action == BBCodeAction.EXPORT && plan.edit == null) {
        "Invalid BBCode export action"
      }
      require(
        plan.units != null &&
          plan.units.size in request.units.size..1_000_000 &&
          plan.units.all { it in 0..65535 }
      ) {
        "Invalid BBCode export units"
      }
    }
    BBCodeOperation.EDIT -> {
      require(plan.units == null && plan.action != BBCodeAction.EXPORT) {
        "Invalid BBCode edit action"
      }
      if (plan.action == BBCodeAction.SPLICE) {
        validateEdit(request, checkNotNull(plan.edit) { "Missing BBCode edit slices" })
      } else {
        require(plan.edit == null) { "Unexpected BBCode edit slices" }
        if (plan.action == BBCodeAction.ORIGINAL) {
          require(request.originUnits == request.units) { "Invalid BBCode original action" }
        }
      }
    }
  }
  return plan
}

private fun validUnits(units: List<Int>): Boolean =
  units.size <= 65_536 && units.all { it in 0..65535 }

private fun validateEdit(request: BBCodeRequest, edit: BBCodeEdit) {
  val origin = checkNotNull(request.originUnits)
  val target = request.units
  require(
    edit.sourceStart in 0..origin.size &&
      edit.sourceEnd in edit.sourceStart..origin.size &&
      edit.targetStart in 0..target.size &&
      edit.targetEnd in edit.targetStart..target.size &&
      edit.sourceStart == edit.targetStart &&
      ((edit.sourceStart == edit.sourceEnd) xor (edit.targetStart == edit.targetEnd)) &&
      origin.size - (edit.sourceEnd - edit.sourceStart) + (edit.targetEnd - edit.targetStart) ==
        target.size
  ) {
    "Invalid BBCode edit slices"
  }
  require(
    origin.subList(0, edit.sourceStart) == target.subList(0, edit.targetStart) &&
      origin.subList(edit.sourceEnd, origin.size) == target.subList(edit.targetEnd, target.size)
  ) {
    "Invalid BBCode edit text"
  }
}

private external fun nativePlanBBCodeV1(request: ByteBuffer, size: Int, response: ByteBuffer): Int
