@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Closed V1 operations owned by `eh_net::cache_keys`. */
@Serializable
internal enum class CacheKeyKind {
  @SerialName("image") IMAGE,
  @SerialName("thumb") THUMB,
  @SerialName("preview") PREVIEW,
  @SerialName("url") URL,
}

/** V1 key observations and the selected site snapshot. */
@Serializable
internal data class CacheKeyRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val kind: CacheKeyKind,
  val input: String? = null,
  val gid: Long? = null,
  val index: Int? = null,
  val exHentai: Boolean? = null,
)

/** V1 complete, operation-tagged key result. */
@Serializable internal data class CacheKeyPlan(val kind: CacheKeyKind, val value: String)

/** Validates V1 envelope/operation before Coil or image caches consume the value. */
internal fun planCacheKey(
  request: CacheKeyRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanCacheKeyV1,
): String {
  val envelope =
    invokeRustCbor<ApiEnvelope<CacheKeyPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Cache key",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "cacheKey") {
    "Unexpected cache key contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing cache key payload" }
  require(plan.kind == request.kind && plan.value.length <= 16 * 1024 + 64) {
    "Invalid cache key result"
  }
  return plan.value
}

/** V1 `eh_net::cache_keys` transformation. */
private external fun nativePlanCacheKeyV1(request: ByteBuffer, size: Int, response: ByteBuffer): Int
