@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 scheduling/cache facts owned by `eh_domain::recovery_search`. */
@Serializable
internal data class RecoverySearchRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val cached: Boolean,
  val elapsedMillis: Long,
  val cacheSize: Int,
  val hasResults: Boolean = false,
)

/** V1 complete scheduler intents; gallery models and network effects stay Kotlin-owned. */
@Serializable
internal data class RecoverySearchPlan(
  val useCache: Boolean,
  val waitMillis: Long,
  val store: Boolean,
  val evictCount: Int,
)

/** Validates the whole plan before delay, HTTP or cache mutation. */
internal fun planRecoverySearch(
  request: RecoverySearchRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanRecoverySearchV1,
): RecoverySearchPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<RecoverySearchPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Recovery search",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "recoverySearch") {
    "Unexpected recovery search contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing recovery search payload" }
  require(
    plan.useCache == request.cached &&
      plan.waitMillis in 0..3_000 &&
      plan.evictCount in 0..1 &&
      (!plan.store || request.hasResults) &&
      (!plan.useCache || (plan.waitMillis == 0L && !plan.store && plan.evictCount == 0)) &&
      (plan.store || plan.evictCount == 0) &&
      (plan.evictCount == 0 || request.cacheSize > 0)
  ) {
    "Invalid recovery search plan"
  }
  return plan
}

/** V1 native scheduler owner. */
private external fun nativePlanRecoverySearchV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
