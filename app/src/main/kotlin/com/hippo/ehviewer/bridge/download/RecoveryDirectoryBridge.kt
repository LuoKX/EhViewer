@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.RecoveryFileKind
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class RecoveryDirectoryOperation {
  @SerialName("observe") OBSERVE,
  @SerialName("promote") PROMOTE,
}

@Serializable
internal data class RecoveryDirectoryChild(
  @ByteString val sourceIdUtf16: ByteArray,
  @ByteString val nameUtf16: ByteArray,
  val size: Long?,
  val modifiedAtMillis: Long?,
)

@Serializable
internal data class RecoveryDirectoryPromotion(
  @ByteString val relativePathUtf16: ByteArray,
  val kind: RecoveryFileKind,
  val gid: Long?,
)

@Serializable
internal data class RecoveryDirectoryRequest(
  val version: Int,
  val operation: RecoveryDirectoryOperation,
  val children: List<RecoveryDirectoryChild>,
  val promotions: List<RecoveryDirectoryPromotion>,
)

@Serializable
internal enum class RecoveryPathDecision {
  @SerialName("none") NONE,
  @SerialName("relative") RELATIVE,
  @SerialName("parent") PARENT,
}

@Serializable
internal data class RecoveryDirectoryPromotionPlan(
  val index: Int,
  val decision: RecoveryPathDecision,
  val end: Int,
)

@Serializable
internal data class RecoveryDirectoryPlan(
  val indices: List<Int>,
  val spiderIndex: Int?,
  val comicIndex: Int?,
  val totalSize: Long,
  val modifiedAtMillis: Long?,
  val promotions: List<RecoveryDirectoryPromotionPlan>,
)

/** One bounded batch. Validate entire returned plan before sidecar reads or path effects. */
internal fun planRecoveryDirectory(
  request: RecoveryDirectoryRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanRecoveryDirectoryV1,
): RecoveryDirectoryPlan {
  require(
    request.version == 1 &&
      request.children.size <= 100_000 &&
      request.promotions.size <= 100_000 &&
      (request.operation == RecoveryDirectoryOperation.OBSERVE || request.children.isEmpty()) &&
      (request.operation == RecoveryDirectoryOperation.PROMOTE || request.promotions.isEmpty())
  ) {
    "Invalid recovery directory request"
  }
  val texts =
    request.children.asSequence().flatMap { sequenceOf(it.sourceIdUtf16, it.nameUtf16) } +
      request.promotions.asSequence().map { it.relativePathUtf16 }
  require(texts.all { it.size % 2 == 0 } && texts.sumOf { it.size.toLong() } <= 16 * 1024 * 1024) {
    "Invalid recovery directory observations"
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size in 1..32 * 1024 * 1024) { "Invalid recovery directory request size" }
  val count =
    if (request.operation == RecoveryDirectoryOperation.OBSERVE) {
      request.children.size
    } else {
      request.promotions.size
    }
  val capacity = Math.addExact(512, Math.multiplyExact(count, 64))
  val input =
    ByteBuffer.allocateDirect(encoded.size).apply {
      put(encoded)
      flip()
    }
  val output = ByteBuffer.allocateDirect(capacity)
  val size = nativeCall(input, encoded.size, output)
  require(size in 1..capacity) { "Invalid recovery directory response size" }
  val bytes =
    ByteArray(size).also {
      output.position(0)
      output.get(it)
    }
  val envelope =
    try {
      rustCbor.decodeFromByteArray<ApiEnvelope<RecoveryDirectoryPlan>>(bytes)
    } catch (_: SerializationException) {
      // Decoder diagnostics can include unknown enum strings. Never reflect wire payloads.
      throw IllegalArgumentException("Invalid recovery directory response")
    }
  require(envelope.version == 1 && envelope.module == "recoveryDirectory") {
    "Invalid recovery directory envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing recovery directory payload" }
  when (request.operation) {
    RecoveryDirectoryOperation.OBSERVE -> {
      require(
        plan.promotions.isEmpty() &&
          plan.indices.size == request.children.size &&
          plan.indices.distinct().size == plan.indices.size &&
          plan.indices.all { it in request.children.indices } &&
          (plan.spiderIndex == null || plan.spiderIndex in request.children.indices) &&
          (plan.comicIndex == null || plan.comicIndex in request.children.indices) &&
          (plan.modifiedAtMillis == null) == request.children.all { it.modifiedAtMillis == null }
      ) {
        "Invalid recovery directory observation plan"
      }
      // Check source reference shape, not legacy sorting/selection or aggregation parity.
      plan.spiderIndex?.let {
        require(request.children[it].nameUtf16.matchesSidecar(".ehviewer")) {
          "Invalid recovery directory sidecar slot"
        }
      }
      plan.comicIndex?.let {
        require(request.children[it].nameUtf16.matchesSidecar("ComicInfo.xml")) {
          "Invalid recovery directory sidecar slot"
        }
      }
      require(
        plan.modifiedAtMillis == null ||
          request.children.any { it.modifiedAtMillis == plan.modifiedAtMillis }
      ) {
        "Invalid recovery directory timestamp"
      }
    }

    RecoveryDirectoryOperation.PROMOTE -> {
      require(
        plan.indices.isEmpty() &&
          plan.spiderIndex == null &&
          plan.comicIndex == null &&
          plan.totalSize == 0L &&
          plan.modifiedAtMillis == null &&
          plan.promotions.size == request.promotions.size
      ) {
        "Invalid recovery directory promotion plan"
      }
      plan.promotions.forEachIndexed { index, path ->
        val source = request.promotions[index]
        val length = source.relativePathUtf16.size / 2
        require(path.index == index && path.end in 0..length) {
          "Invalid recovery directory promotion slot"
        }
        when (path.decision) {
          RecoveryPathDecision.NONE -> {
            require(path.end == 0) { "Invalid ineligible recovery path" }
          }

          RecoveryPathDecision.RELATIVE -> {
            require(
              source.gid != null &&
                source.gid > 0 &&
                source.kind == RecoveryFileKind.DIRECTORY &&
                path.end == length &&
                length > 0 &&
                source.relativePathUtf16.decodeUtf16().isNotBlank()
            ) {
              "Invalid relative recovery path"
            }
          }

          RecoveryPathDecision.PARENT -> {
            require(
              source.gid != null &&
                source.gid > 0 &&
                source.kind in listOf(RecoveryFileKind.CBZ, RecoveryFileKind.ZIP) &&
                path.end in 1 until length &&
                source.relativePathUtf16[path.end * 2] == '/'.code.toByte() &&
                source.relativePathUtf16[path.end * 2 + 1] == 0.toByte() &&
                (path.end + 1 until length).none {
                  source.relativePathUtf16[it * 2] == '/'.code.toByte() &&
                    source.relativePathUtf16[it * 2 + 1] == 0.toByte()
                } &&
                source.relativePathUtf16.copyOfRange(0, path.end * 2).decodeUtf16().isNotBlank() &&
                source.relativePathUtf16
                  .copyOfRange((path.end + 1) * 2, source.relativePathUtf16.size)
                  .matchesSidecar(
                    "${source.gid}.${if (source.kind == RecoveryFileKind.CBZ) "cbz" else "zip"}"
                  )
            ) {
              "Invalid parent recovery path"
            }
          }
        }
      }
    }
  }
  return plan
}

private fun ByteArray.matchesSidecar(expected: String): Boolean =
  decodeUtf16().equals(expected, ignoreCase = true)

private fun ByteArray.decodeUtf16(): String =
  CharArray(size / 2) {
      ((this[it * 2].toInt() and 255) or ((this[it * 2 + 1].toInt() and 255) shl 8)).toChar()
    }
    .concatToString()

private external fun nativePlanRecoveryDirectoryV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
