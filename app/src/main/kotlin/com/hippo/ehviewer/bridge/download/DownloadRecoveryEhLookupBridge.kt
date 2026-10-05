package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 complete pure EH recovery operation owned by `eh_domain::recovery_eh_lookup`. */
@Serializable
internal enum class RecoveryEhKind {
  @SerialName("lookup") LOOKUP,
  @SerialName("select") SELECT,
  @SerialName("validateIdentity") VALIDATE_IDENTITY,
  @SerialName("apply") APPLY,
  @SerialName("queryFailure") QUERY_FAILURE,
  @SerialName("failure") FAILURE,
}

/** V1 HTTP-result selection policy. */
@Serializable
internal enum class RecoveryEhSelection {
  @SerialName("title") TITLE,
  @SerialName("gid") GID,
  @SerialName("identity") IDENTITY,
}

/** V1 non-sensitive candidate observations; complete UI models remain Android-owned. */
@Serializable
internal data class RecoveryEhCandidate(val gid: Long, val token: String, val title: String?)

/** V1 closed platform promotion facts. Arbitrary Throwable messages never enter Rust. */
@Serializable
internal enum class RecoveryPromotionSignal {
  SOURCE_MISSING,
  IDENTITY_CONFLICT,
  PAGE_COUNT_CONFLICT,
  PATH_CONFLICT,
  MAPPING_CONFLICT,
  ACTIVE_DOWNLOAD_CONFLICT,
  FORMAL_IDENTITY_CONFLICT,
  LOCAL_PROMOTION_FAILED,
}

/** V1 facts for one operation, without credentials, Room objects or exception messages. */
@Serializable
internal data class RecoveryEhRequest(
  val version: Int,
  val kind: RecoveryEhKind,
  val gid: Long? = null,
  val token: String? = null,
  val title: String? = null,
  val displayName: String = "",
  val hasPageToken: Boolean = false,
  val pageIndex: Int = 0,
  val selection: RecoveryEhSelection? = null,
  val candidates: List<RecoveryEhCandidate> = emptyList(),
  val issues: String = "",
  val evidence: String = "",
  val localPages: Int = 0,
  val declaredPages: Int = 0,
  val knownGid: Long? = null,
  val usablePath: Boolean = false,
  val causeTypes: List<String> = emptyList(),
  val promotionSignals: List<RecoveryPromotionSignal> = emptyList(),
  val latestEvidence: String? = null,
  val latestIssues: String? = null,
  val currentStatus: String? = null,
  val notFound: Boolean = false,
)

/** V1 ordered lookup action; Android owns execution and caching. */
@Serializable
internal data class RecoveryEhStep(val kind: String, val key: String, val query: String?)

/** V1 complete decision returned before any Android persistence effect. */
@Serializable
internal data class RecoveryEhPlan(
  val kind: RecoveryEhKind,
  val gid: Long?,
  val title: String?,
  val steps: List<RecoveryEhStep>,
  val selected: List<Int>,
  val status: String?,
  val issues: String?,
  val evidence: String?,
  val promotionFailure: Boolean,
)

/**
 * Calls one Rust owner; CBOR/JNI/linkage/contract failures propagate without replay or fallback.
 */
internal fun planRecoveryEhLookup(
  request: RecoveryEhRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanRecoveryEhLookupV1,
): RecoveryEhPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<RecoveryEhPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "EH recovery",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "recoveryEhLookup") {
    "Unexpected EH recovery contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing EH recovery payload" }
  require(plan.kind == request.kind) { "Unexpected EH recovery operation" }
  when (request.kind) {
    RecoveryEhKind.LOOKUP ->
      require(
        plan.title != null &&
          plan.steps.all {
            it.kind in LOOKUP_STEPS && (it.kind !in SEARCH_STEPS || it.query != null)
          }
      ) {
        "Invalid EH lookup steps"
      }
    RecoveryEhKind.SELECT ->
      require(
        plan.selected.size <= 25 &&
          plan.selected.distinct().size == plan.selected.size &&
          plan.selected.all { it in request.candidates.indices }
      ) {
        "Invalid EH selection indices"
      }
    RecoveryEhKind.APPLY ->
      require(plan.status in STATUS_NAMES && plan.issues != null && plan.evidence != null) {
        "Invalid EH apply plan"
      }
    RecoveryEhKind.QUERY_FAILURE,
    RecoveryEhKind.FAILURE ->
      require(plan.status in STATUS_NAMES && plan.issues != null) { "Invalid EH failure plan" }
    RecoveryEhKind.VALIDATE_IDENTITY -> Unit
  }
  return plan
}

/** V1 `eh_domain::recovery_eh_lookup` direct-buffer planner. */
private external fun nativePlanRecoveryEhLookupV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int

private val LOOKUP_STEPS = setOf("pageToken", "storedToken", "title", "gid")
private val SEARCH_STEPS = setOf("title", "gid")
private val STATUS_NAMES = DownloadRecoveryStatus.entries.map { it.name }.toSet()
