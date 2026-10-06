@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.updater

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.io.IOException
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.encodeToByteArray
import tachiyomi.data.release.GithubCommit
import tachiyomi.data.release.GithubWorkflowRun

internal const val UPDATE_MAX_ITEMS = 4096
internal const val UPDATE_MAX_TEXT = 65_536
internal const val UPDATE_MAX_JSON = 2 * 1024 * 1024
internal const val UPDATE_MAX_REQUEST = 3 * 1024 * 1024

internal typealias UpdateNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

@Serializable
internal enum class UpdateOperation {
  @SerialName("check") CHECK,
  @SerialName("match") MATCH,
  @SerialName("prepareRelease") PREPARE_RELEASE,
  @SerialName("prepareArtifacts") PREPARE_ARTIFACTS,
  @SerialName("selectRelease") SELECT_RELEASE,
  @SerialName("selectArtifacts") SELECT_ARTIFACTS,
  @SerialName("selectWorkflow") SELECT_WORKFLOW,
  @SerialName("compareRelease") COMPARE_RELEASE,
  @SerialName("compareWorkflow") COMPARE_WORKFLOW,
}

@Serializable
internal enum class UpdateResponseOperation {
  @SerialName("policy") POLICY,
  @SerialName("repo") REPO,
  @SerialName("release") RELEASE,
  @SerialName("workflow") WORKFLOW,
  @SerialName("artifacts") ARTIFACTS,
  @SerialName("comparison") COMPARISON,
}

@Serializable
internal enum class UpdateDecision {
  @SerialName("skip") SKIP,
  @SerialName("check") CHECK,
  @SerialName("match") MATCH,
  @SerialName("observe") OBSERVE,
  @SerialName("selected") SELECTED,
  @SerialName("noUpdate") NO_UPDATE,
  @SerialName("candidate") CANDIDATE,
}

/** Immutable facts only. Credentials/headers/settings objects never cross JNI. */
@Serializable
internal data class UpdatePolicyRequest(
  @EncodeDefault val version: Int = 1,
  val operation: UpdateOperation,
  @EncodeDefault val nowSeconds: Long = 0,
  @EncodeDefault val nowNanos: Int = 0,
  @EncodeDefault val lastSeconds: Long = 0,
  @EncodeDefault val intervalDays: Int = 0,
  @EncodeDefault val forceCheck: Boolean = false,
  @EncodeDefault val flavor: String = "",
  @EncodeDefault val abis: List<String> = emptyList(),
  @EncodeDefault val names: List<String> = emptyList(),
  @EncodeDefault val matches: List<Boolean>? = null,
  @EncodeDefault val current: String = "",
  @EncodeDefault val latest: String = "",
)

@Serializable
internal data class UpdateRequest(
  @EncodeDefault val version: Int = 1,
  val operation: UpdateResponseOperation,
  val policy: UpdatePolicyRequest,
  @EncodeDefault val json: String? = null,
)

@Serializable
internal data class UpdatePolicyPlan(
  val decision: UpdateDecision,
  val index: Int?,
  val versionUnits: List<Int>?,
  val matched: Boolean?,
  val order: List<Int>?,
) {
  val version: String?
    get() = versionUnits?.let { values ->
      CharArray(values.size) { values[it].toChar() }.concatToString()
    }
}

@Serializable
internal data class UpdatePlan(
  val policy: UpdatePolicyPlan?,
  val branch: String?,
  val workflowRun: GithubWorkflowRun?,
  val info: String?,
  val downloadLink: String?,
  val selectedIndex: Int?,
  val sourceCount: Int?,
  val commits: List<GithubCommit>?,
)

/** One synchronous operation, fully validated before persistence/HTTP/object application. */
internal fun planUpdate(
  request: UpdateRequest,
  nativeCall: UpdateNativeCall = ::nativePlanUpdateV1,
): UpdatePlan {
  validateInput(request)
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size <= UPDATE_MAX_REQUEST) { "Invalid update request size" }
  val envelope =
    try {
      invokeRustCbor<ApiEnvelope<UpdatePlan>>(encoded, "Update", nativeCall)
    } catch (_: SerializationException) {
      // Decoder diagnostics can contain untrusted enum/text values; never expose them or a cause.
      throw IOException("Invalid update response")
    }
  require(envelope.version == 1 && envelope.module == "updater") { "Invalid update envelope" }
  require((envelope.payload == null) != (envelope.error == null)) {
    "Invalid update envelope shape"
  }
  envelope.error?.let {
    require(it.kind == RustErrorKind.ERROR && it.message in UPDATE_ERRORS) {
      "Invalid update error"
    }
    throw IOException(it.message)
  }
  val plan = checkNotNull(envelope.payload) { "Missing update payload" }
  validatePlan(request, plan)
  return plan
}

internal fun planUpdatePolicy(
  request: UpdatePolicyRequest,
  nativeCall: UpdateNativeCall = ::nativePlanUpdateV1,
): UpdatePolicyPlan =
  checkNotNull(
    planUpdate(
        UpdateRequest(operation = UpdateResponseOperation.POLICY, policy = request),
        nativeCall,
      )
      .policy
  )

private fun validateInput(request: UpdateRequest) {
  val facts = request.policy
  require(
    request.version == 1 &&
      facts.version == 1 &&
      facts.nowSeconds in MIN_SECOND..MAX_SECOND &&
      facts.nowNanos in 0..999_999_999 &&
      facts.names.size <= UPDATE_MAX_ITEMS &&
      facts.abis.size <= 32 &&
      (facts.matches == null || facts.matches.size <= facts.names.size) &&
      (listOf(
          facts.flavor,
          facts.current,
          facts.latest,
        ) + facts.names + facts.abis)
        .all(::boundedText) &&
      (request.operation == UpdateResponseOperation.POLICY) == (request.json == null) &&
      (request.json == null || request.json.encodeToByteArray().size <= UPDATE_MAX_JSON)
  ) {
    "Invalid update policy input"
  }
}

private fun validatePlan(
  request: UpdateRequest,
  plan: UpdatePlan,
) {
  require(
    listOfNotNull(plan.branch, plan.info, plan.downloadLink).all(::boundedText) &&
      (plan.selectedIndex == null) == (plan.sourceCount == null) &&
      (plan.sourceCount == null || plan.sourceCount in 1..UPDATE_MAX_ITEMS) &&
      (plan.selectedIndex == null || plan.selectedIndex in 0 until checkNotNull(plan.sourceCount))
  ) {
    "Invalid update plan bounds"
  }
  val policy = plan.policy
  require(
    when (request.operation) {
      UpdateResponseOperation.POLICY -> {
        policy != null &&
          plan.branch == null &&
          plan.workflowRun == null &&
          plan.info == null &&
          plan.downloadLink == null &&
          plan.selectedIndex == null &&
          plan.commits == null
      }

      UpdateResponseOperation.REPO -> {
        policy == null &&
          plan.branch != null &&
          plan.workflowRun == null &&
          plan.info == null &&
          plan.downloadLink == null &&
          plan.selectedIndex == null &&
          plan.commits == null
      }

      UpdateResponseOperation.COMPARISON -> {
        policy == null &&
          plan.branch == null &&
          plan.workflowRun == null &&
          plan.info == null &&
          plan.downloadLink == null &&
          plan.selectedIndex == null &&
          plan.commits != null &&
          plan.commits.size <= UPDATE_MAX_ITEMS &&
          plan.commits.all { boundedText(it.commit.message) && boundedText(it.commit.author.name) }
      }

      UpdateResponseOperation.RELEASE -> {
        policy != null &&
          plan.branch == null &&
          plan.workflowRun == null &&
          plan.info != null &&
          plan.downloadLink != null &&
          plan.selectedIndex != null &&
          plan.commits == null
      }

      UpdateResponseOperation.WORKFLOW -> {
        policy != null &&
          plan.branch == null &&
          plan.workflowRun != null &&
          plan.info == null &&
          plan.downloadLink == null &&
          plan.selectedIndex == 0 &&
          plan.commits == null &&
          listOf(
              plan.workflowRun.headSha,
              plan.workflowRun.title,
              plan.workflowRun.artifactsUrl,
            )
            .all(::boundedText)
      }

      UpdateResponseOperation.ARTIFACTS -> {
        policy != null &&
          plan.branch == null &&
          plan.workflowRun == null &&
          plan.info == null &&
          plan.downloadLink != null &&
          plan.selectedIndex != null &&
          plan.commits == null
      }
    }
  ) {
    "Invalid update plan shape"
  }
  if (policy != null) {
    val operation =
      when (request.operation) {
        UpdateResponseOperation.RELEASE -> UpdateOperation.COMPARE_RELEASE
        UpdateResponseOperation.WORKFLOW -> UpdateOperation.COMPARE_WORKFLOW
        UpdateResponseOperation.ARTIFACTS -> UpdateOperation.SELECT_ARTIFACTS
        else -> request.policy.operation
      }
    val namesCount =
      if (request.operation == UpdateResponseOperation.POLICY) {
        request.policy.names.size
      } else {
        checkNotNull(plan.sourceCount)
      }
    validatePolicy(request.policy, operation, namesCount, policy)
    if (request.operation == UpdateResponseOperation.ARTIFACTS) {
      require(policy.index == plan.selectedIndex) { "Invalid update selection association" }
    }
    if (request.operation == UpdateResponseOperation.WORKFLOW) {
      val sha = checkNotNull(plan.workflowRun).headSha
      require(policy.versionUnits == sha.take(7).map(Char::code)) {
        "Invalid update SHA association"
      }
    }
  }
}

private fun validatePolicy(
  request: UpdatePolicyRequest,
  operation: UpdateOperation,
  count: Int,
  plan: UpdatePolicyPlan,
) {
  require(
    plan.versionUnits == null ||
      (plan.versionUnits.size <= UPDATE_MAX_TEXT && plan.versionUnits.all { it in 0..65535 })
  ) {
    "Invalid update version units"
  }
  require(
    (plan.order != null) ==
      (operation in listOf(UpdateOperation.PREPARE_RELEASE, UpdateOperation.PREPARE_ARTIFACTS))
  ) {
    "Invalid update predicate order shape"
  }
  require(
    when (operation) {
      UpdateOperation.PREPARE_RELEASE,
      UpdateOperation.PREPARE_ARTIFACTS -> {
        val order = checkNotNull(plan.order)
        plan.decision == UpdateDecision.OBSERVE &&
          plan.index == null &&
          plan.versionUnits == null &&
          plan.matched == null &&
          order.size == count &&
          count > 0 &&
          order.all { it in 0 until count } &&
          order.toSet().size == count
      }
      UpdateOperation.CHECK -> {
        plan.decision in listOf(UpdateDecision.CHECK, UpdateDecision.SKIP) &&
          plan.index == null &&
          plan.versionUnits == null &&
          plan.matched == null
      }

      UpdateOperation.MATCH -> {
        plan.decision == UpdateDecision.MATCH &&
          plan.index == null &&
          plan.versionUnits == null &&
          plan.matched != null &&
          request.names.size == 1
      }

      UpdateOperation.SELECT_RELEASE,
      UpdateOperation.SELECT_ARTIFACTS,
      UpdateOperation.SELECT_WORKFLOW -> {
        plan.decision == UpdateDecision.SELECTED &&
          plan.index != null &&
          plan.index in 0 until count &&
          (operation != UpdateOperation.SELECT_WORKFLOW || plan.index == 0) &&
          plan.versionUnits == null &&
          plan.matched == null
      }

      UpdateOperation.COMPARE_RELEASE,
      UpdateOperation.COMPARE_WORKFLOW -> {
        plan.decision in listOf(UpdateDecision.CANDIDATE, UpdateDecision.NO_UPDATE) &&
          plan.index == null &&
          plan.matched == null &&
          plan.versionUnits != null &&
          (operation != UpdateOperation.COMPARE_WORKFLOW || plan.versionUnits.size <= 7) &&
          ((plan.decision == UpdateDecision.NO_UPDATE) == (plan.version == request.current))
      }
    }
  ) {
    "Invalid update policy plan"
  }
}

private fun boundedText(value: String) = value.encodeToByteArray().size <= UPDATE_MAX_TEXT

private const val MIN_SECOND = -31_557_014_167_219_200L
private const val MAX_SECOND = 31_556_889_864_403_199L
private val UPDATE_ERRORS =
  setOf(
    "Invalid update policy input",
    "Invalid update response",
    "No update release assets available",
    "No update workflow artifacts available",
    "No successful update workflow runs available",
  )

private external fun nativePlanUpdateV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
