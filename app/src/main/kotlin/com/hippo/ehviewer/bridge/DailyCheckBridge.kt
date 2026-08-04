package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Versioned sanitized facts captured for one daily-check operation. */
@Serializable
internal data class DailyCheckRequest(
  val version: Int,
  @SerialName("signedIn") val signedIn: Boolean,
  @SerialName("lastDawnDays") val lastDawnDays: Int,
  @SerialName("nowEpochSeconds") val nowEpochSeconds: Long,
)

/** Complete pure decision selected before any network or notification effects. */
@Serializable
internal data class DailyCheckPlan(
  val today: Int,
  @SerialName("shouldCheck") val shouldCheck: Boolean,
)

/** Preserves the legacy pure rule when the owning operation selected Kotlin. */
internal fun planLegacyDailyCheck(request: DailyCheckRequest): DailyCheckPlan {
  val today = (request.nowEpochSeconds / SECONDS_PER_DAY).toInt()
  return DailyCheckPlan(
    today = today,
    shouldCheck = request.signedIn && request.lastDawnDays != today,
  )
}

/** Calls Rust once and validates the versioned response envelope. */
internal fun planDailyCheck(
  request: DailyCheckRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanDailyCheckV1,
): DailyCheckPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<DailyCheckPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Daily Check",
      nativeCall = nativeCall,
    )
  require(envelope.version == DAILY_CHECK_API_VERSION) {
    "Unsupported Daily Check API version"
  }
  require(envelope.module == DAILY_CHECK_MODULE) {
    "Unexpected Daily Check module"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing Daily Check payload" }
  require(!plan.shouldCheck || request.signedIn) {
    "Daily Check cannot run without an authenticated session"
  }
  require(!plan.shouldCheck || plan.today != request.lastDawnDays) {
    "Daily Check cannot repeat an already completed UTC day"
  }
  return plan
}

/** Runs exactly one planner for the backend captured by the owning operation. */
internal fun selectDailyCheckPlan(
  backend: CoreBackend,
  legacyPlanner: () -> DailyCheckPlan,
  rustPlanner: () -> DailyCheckPlan,
): DailyCheckPlan =
  when (backend) {
    CoreBackend.LEGACY -> legacyPlanner()
    CoreBackend.RUST -> rustPlanner()
  }

private external fun nativePlanDailyCheckV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val DAILY_CHECK_API_VERSION = 1
internal const val DAILY_CHECK_MODULE = "dailyCheck"
private const val SECONDS_PER_DAY = 86_400L
