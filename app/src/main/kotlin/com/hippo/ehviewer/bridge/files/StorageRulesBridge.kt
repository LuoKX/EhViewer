@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.files

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class StorageOperation {
  @SerialName("classify") CLASSIFY,
  @SerialName("candidates") CANDIDATES,
  @SerialName("names") NAMES,
  @SerialName("cleanup") CLEANUP,
}

@Serializable
internal enum class RecoveryFileKind {
  @SerialName("directory") DIRECTORY,
  @SerialName("cbz") CBZ,
  @SerialName("zip") ZIP,
  @SerialName("rar") RAR,
  @SerialName("sevenZip") SEVEN_ZIP,
  @SerialName("tar") TAR,
  @SerialName("tarGz") TAR_GZ,
  @SerialName("tarXz") TAR_XZ,
}

@Serializable
internal data class RecoveryFileObservation(
  val index: Int,
  val directory: Boolean,
  val root: Boolean,
  val hasStrictPage: Boolean,
  @ByteString val suffixUtf16: ByteArray,
)

@Serializable internal data class RecoveryCandidateRef(val index: Int, val kind: RecoveryFileKind)

@Serializable
internal data class StorageCandidateInput(
  val index: Int,
  val kind: RecoveryFileKind,
  @ByteString val sourceIdUtf16: ByteArray,
)

@Serializable internal data class StorageNameInput(@ByteString val utf16: ByteArray)

@Serializable
internal data class StorageRulesRequest(
  @EncodeDefault val version: Int = 1,
  val operation: StorageOperation,
  @EncodeDefault val observations: List<RecoveryFileObservation> = emptyList(),
  @EncodeDefault val candidates: List<StorageCandidateInput> = emptyList(),
  @EncodeDefault val names: List<StorageNameInput> = emptyList(),
  @EncodeDefault val modified: List<Long> = emptyList(),
  @EncodeDefault val maxItems: Int = 0,
)

@Serializable
internal data class StorageRulesPlan(
  val indices: List<Int>,
  val candidates: List<RecoveryCandidateRef>,
  val limitExceeded: Boolean,
)

/** Raw Java String order, including lone surrogates; not Charset replacement encoding. */
internal fun String.toUtf16LeBytes(): ByteArray =
  ByteArray(Math.multiplyExact(length, 2)).also {
    for (index in indices) {
      it[index * 2] = this[index].code.toByte()
      it[index * 2 + 1] = (this[index].code ushr 8).toByte()
    }
  }

/**
 * Validate complete plan before effects. Response bound depends on output facts, not input paths.
 */
internal fun planStorageRules(
  request: StorageRulesRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanStorageRulesV1,
): StorageRulesPlan {
  require(request.operation != StorageOperation.CANDIDATES || request.maxItems >= 0) {
    "Requested element count ${request.maxItems} is less than zero."
  }
  require(
    request.version == 1 &&
      request.observations.size <= 1024 &&
      request.candidates.size <= 100_000 &&
      request.names.size <= 100_000 &&
      request.modified.size <= 100_000 &&
      (request.operation == StorageOperation.CLASSIFY || request.observations.isEmpty()) &&
      (request.operation == StorageOperation.CANDIDATES || request.candidates.isEmpty()) &&
      (request.operation == StorageOperation.NAMES || request.names.isEmpty()) &&
      (request.operation == StorageOperation.CLEANUP || request.modified.isEmpty()) &&
      (request.operation !in listOf(StorageOperation.CLASSIFY, StorageOperation.NAMES) ||
        request.maxItems == 0)
  ) {
    "Invalid storage rule request"
  }
  require(
    request.observations.all {
      it.index in 0 until 100_000 && it.suffixUtf16.size <= 14 && it.suffixUtf16.size % 2 == 0
    }
  ) {
    "Invalid recovery file observation"
  }
  require(request.candidates.all { it.index in 0 until 100_000 }) {
    "Invalid recovery candidate slot"
  }
  require(
    request.observations.map { it.index }.distinct().size == request.observations.size &&
      request.candidates.map { it.index }.distinct().size == request.candidates.size
  ) {
    "Duplicate storage observation slot"
  }
  val names =
    request.names.asSequence().map { it.utf16 } +
      request.candidates.asSequence().map { it.sourceIdUtf16 }
  require(names.all { it.size % 2 == 0 } && names.sumOf { it.size.toLong() } <= 16 * 1024 * 1024) {
    "Invalid storage name observations"
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size in 1..32 * 1024 * 1024) { "Invalid storage rule request size" }
  val count =
    when (request.operation) {
      StorageOperation.CLASSIFY -> request.observations.size
      StorageOperation.CANDIDATES -> minOf(request.maxItems, request.candidates.size)
      StorageOperation.NAMES -> request.names.size
      StorageOperation.CLEANUP -> request.modified.size
    }
  val capacity = Math.addExact(512, Math.multiplyExact(count, 64))
  val input =
    ByteBuffer.allocateDirect(encoded.size).apply {
      put(encoded)
      flip()
    }
  val output = ByteBuffer.allocateDirect(capacity)
  val size = nativeCall(input, encoded.size, output)
  require(size in 1..capacity) { "Invalid storage rule response size" }
  val bytes =
    ByteArray(size).also {
      output.position(0)
      output.get(it)
    }
  val envelope = rustCbor.decodeFromByteArray<ApiEnvelope<StorageRulesPlan>>(bytes)
  require(envelope.version == 1 && envelope.module == "storageRules") {
    "Invalid storage rule envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing storage rule payload" }
  validateStoragePlan(request, plan)
  return plan
}

private fun validateStoragePlan(request: StorageRulesRequest, plan: StorageRulesPlan) {
  require(
    plan.indices.distinct().size == plan.indices.size &&
      plan.candidates.map { it.index }.distinct().size == plan.candidates.size
  ) {
    "Duplicate storage plan slot"
  }
  when (request.operation) {
    StorageOperation.CLASSIFY -> {
      val observations = request.observations.associateBy { it.index }
      require(
        plan.indices.isEmpty() &&
          !plan.limitExceeded &&
          plan.candidates.all { candidate ->
            val observation = observations[candidate.index]
            observation != null &&
              if (candidate.kind == RecoveryFileKind.DIRECTORY) {
                observation.directory && !observation.root && observation.hasStrictPage
              } else {
                !observation.directory
              }
          }
      ) {
        "Invalid storage classification plan"
      }
    }
    StorageOperation.CANDIDATES -> {
      val candidates = request.candidates.associateBy { it.index }
      require(
        plan.indices.isEmpty() &&
          plan.candidates.size == minOf(request.maxItems, request.candidates.size) &&
          plan.limitExceeded == (request.candidates.size > request.maxItems) &&
          plan.candidates.all { candidates[it.index]?.kind == it.kind }
      ) {
        "Invalid recovery candidate plan"
      }
    }
    StorageOperation.NAMES,
    StorageOperation.CLEANUP -> {
      val size =
        if (request.operation == StorageOperation.NAMES) request.names.size
        else request.modified.size
      val expected =
        if (request.operation == StorageOperation.NAMES) size
        else (size.toLong() - maxOf(request.maxItems, 0)).coerceAtLeast(0).toInt()
      require(
        plan.candidates.isEmpty() &&
          !plan.limitExceeded &&
          plan.indices.size == expected &&
          plan.indices.all { it in 0 until size }
      ) {
        "Invalid storage index plan"
      }
    }
  }
}

private external fun nativePlanStorageRulesV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
