@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

@Serializable
internal data class RecoveryMappingCandidate(
  val gid: Long?,
  val status: RecoveryStatus,
  val sourceKey: String,
  @ByteString val pathUtf16: ByteArray?,
)

@Serializable
internal data class RecoveryMappingDownload(val gid: Long, @ByteString val dirnameUtf16: ByteArray?)

@Serializable
internal data class RecoveryMappingRequest(
  val version: Int,
  val candidates: List<RecoveryMappingCandidate>,
  val downloads: List<RecoveryMappingDownload>,
)

@Serializable internal data class RecoveryMapping(val candidateIndex: Int?, val sourceKey: String)

@Serializable internal data class RecoveryMappingPlan(val mappings: List<RecoveryMapping>)

/** Validate the whole association before building the existing conflict request. */
internal fun planRecoveryMappings(
  request: RecoveryMappingRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanRecoveryMappingsV1,
): RecoveryMappingPlan {
  require(
    request.version == 1 && request.candidates.size <= 100_000 && request.downloads.size <= 100_000
  ) {
    "Invalid recovery mapping request"
  }
  require(request.candidates.all { it.sourceKey.isRecoveryKey() }) {
    "Invalid recovery mapping source"
  }
  val paths =
    request.candidates.asSequence().mapNotNull { it.pathUtf16 } +
      request.downloads.asSequence().mapNotNull { it.dirnameUtf16 }
  require(paths.all { it.size % 2 == 0 } && paths.sumOf { it.size.toLong() } <= 16 * 1024 * 1024) {
    "Invalid recovery mapping paths"
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size in 1..32 * 1024 * 1024) { "Invalid recovery mapping request size" }
  val input =
    ByteBuffer.allocateDirect(encoded.size).apply {
      put(encoded)
      flip()
    }
  val capacity = 512 + request.downloads.size * 128
  val output = ByteBuffer.allocateDirect(capacity)
  val size = nativeCall(input, encoded.size, output)
  require(size in 1..capacity) { "Invalid recovery mapping response size" }
  val bytes =
    ByteArray(size).also {
      output.position(0)
      output.get(it)
    }
  val envelope =
    try {
      rustCbor.decodeFromByteArray<ApiEnvelope<RecoveryMappingPlan>>(bytes)
    } catch (_: SerializationException) {
      throw IllegalArgumentException("Invalid recovery mapping response")
    }
  require(envelope.version == 1 && envelope.module == "recoveryMapping") {
    "Invalid recovery mapping envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing recovery mapping payload" }
  require(plan.mappings.size == request.downloads.size) { "Incomplete recovery mapping plan" }
  plan.mappings.forEachIndexed { index, mapping ->
    require(mapping.sourceKey.isRecoveryKey()) { "Invalid recovery mapping identity" }
    mapping.candidateIndex?.let { slot ->
      require(slot in request.candidates.indices) { "Invalid recovery mapping slot" }
      val candidate = request.candidates[slot]
      val download = request.downloads[index]
      require(
        candidate.status == RecoveryStatus.READY &&
          candidate.gid == download.gid &&
          candidate.pathUtf16.contentEquals(download.dirnameUtf16) &&
          candidate.sourceKey == mapping.sourceKey
      ) {
        "Invalid recovery mapping source reference"
      }
    }
  }
  return plan
}

private fun String.isRecoveryKey() = length == 64 && all { it in '0'..'9' || it in 'a'..'f' }

private external fun nativePlanRecoveryMappingsV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
