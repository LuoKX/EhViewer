@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

/**
 * Complete V1 metadata DTO owned by `eh_domain::spider_info`; storage itself remains unversioned.
 */
@Serializable
internal data class SpiderInfoData(
  val gid: Long,
  val token: String,
  val pages: Int,
  val pTokenMap: Map<Int, String> = emptyMap(),
  val previewPerPage: Int = -1,
)

/** Closed V1 complete codec operation. */
@Serializable
internal enum class SpiderInfoCodecKind {
  @SerialName("decode") DECODE,
  @SerialName("encode") ENCODE,
}

/** Bounded V1 codec request. */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
internal data class SpiderInfoCodecRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val kind: SpiderInfoCodecKind,
  @ByteString val bytes: ByteArray? = null,
  val allowLegacy: Boolean? = null,
  val info: SpiderInfoData? = null,
)

/** Bounded V1 result; exactly the operation's field is present. */
@OptIn(ExperimentalSerializationApi::class)
@Serializable
internal data class SpiderInfoCodecPlan(
  val kind: SpiderInfoCodecKind,
  val info: SpiderInfoData? = null,
  @ByteString val bytes: ByteArray? = null,
)

/** Validates the complete native result without Kotlin codec fallback. */
internal fun codecSpiderInfo(
  request: SpiderInfoCodecRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeCodecSpiderInfoV1,
): SpiderInfoCodecPlan {
  request.bytes?.let {
    require(it.size in 1..MAX_SPIDER_INFO_BYTES) { "Invalid SpiderInfo input size" }
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size <= 2 * 1024 * 1024) { "SpiderInfo request exceeds resource budget" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<SpiderInfoCodecPlan>>(
      encoded,
      operation = "SpiderInfo",
      maxResponseSize = 2 * 1024 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "spiderInfo") {
    "Unexpected SpiderInfo contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing SpiderInfo payload" }
  require(plan.kind == request.kind) { "Unexpected SpiderInfo operation" }
  require(
    when (plan.kind) {
      SpiderInfoCodecKind.DECODE -> plan.info != null && plan.bytes == null
      SpiderInfoCodecKind.ENCODE ->
        plan.info == null && plan.bytes != null && plan.bytes.size in 1..MAX_SPIDER_INFO_BYTES
    }
  ) {
    "Invalid SpiderInfo result"
  }
  return plan
}

/** V1 `eh_domain::spider_info` exact-sized codec. */
private external fun nativeCodecSpiderInfoV1(request: ByteBuffer, size: Int): ByteArray

internal const val MAX_SPIDER_INFO_BYTES = 512 * 1024
