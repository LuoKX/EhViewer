@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.files

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class FileRuleKind {
  @SerialName("sanitize") SANITIZE,
  @SerialName("extension") EXTENSION,
  @SerialName("basename") BASENAME,
}

@Serializable
internal data class FileRulesRequest(
  @EncodeDefault val version: Int = 1,
  val kind: FileRuleKind,
  val units: List<Int>?,
)

@Serializable internal data class FileRulesPlan(val kind: FileRuleKind, val units: List<Int>?)

internal fun planFileRules(
  kind: FileRuleKind,
  input: String?,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanFileRulesV1,
): String? {
  require(input == null || input.length <= 65_536) { "Invalid file rules input" }
  val request = FileRulesRequest(kind = kind, units = input?.map { it.code })
  val envelope =
    invokeRustCbor<ApiEnvelope<FileRulesPlan>>(
      rustCbor.encodeToByteArray(request),
      "File Rules",
      nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "fileRules") {
    "Invalid File Rules envelope"
  }
  envelope.error?.let {
    // Preserve the former public helper's empty-name exception without calculating its rule again.
    if (it.kind == RustErrorKind.ERROR && it.message == "Invalid filename") {
      error("Invalid filename")
    }
    throw RustFailureException(it)
  }
  val plan = checkNotNull(envelope.payload) { "Missing File Rules payload" }
  require(plan.kind == kind) { "Unexpected File Rules operation" }
  require(plan.units == null || plan.units.size <= 65_536) { "Invalid File Rules result size" }
  require(plan.units?.all { it in 0..65_535 } != false) { "Invalid File Rules UTF-16" }
  val result = plan.units?.map { it.toChar() }?.toCharArray()?.concatToString()
  if (kind == FileRuleKind.SANITIZE) {
    require(!result.isNullOrEmpty()) { "Missing sanitized filename" }
    require(result.length <= 200 && result.toByteArray().size <= 200) {
      "Invalid sanitized filename budget"
    }
    require(result.none { it == '/' || it == '\\' || it == '\u0000' }) {
      "Invalid sanitized filename component"
    }
  }
  return result
}

private external fun nativePlanFileRulesV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
