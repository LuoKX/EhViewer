package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

internal const val DOWNLOAD_RECOVERY_API_VERSION = 1
private const val DOWNLOAD_RECOVERY_MODULE = "downloadRecovery"
private val SHA_256_HEX = Regex("[0-9a-f]{64}")

@Serializable
internal enum class RecoverySourceKind {
  @SerialName("directory") DIRECTORY,
  @SerialName("archive") ARCHIVE,
}

@Serializable
internal data class RecoverySourceKeyInput(
  @SerialName("rootId") val rootId: String,
  @SerialName("sourceId") val sourceId: String,
  @SerialName("sourceKind") val sourceKind: RecoverySourceKind,
)

@Serializable
internal data class RecoveryFingerprintInput(
  val size: Long? = null,
  @SerialName("modifiedAtMillis") val modifiedAtMillis: Long? = null,
  @SerialName("contentDigest") val contentDigest: ByteArray? = null,
)

@Serializable
internal data class RecoveryEntryInput(
  val name: String,
  val size: Long,
  val regular: Boolean,
  val encrypted: Boolean = false,
  val readable: Boolean = false,
)

@Serializable
internal data class RecoveryArchiveInput(
  val format: String,
  val complete: Boolean,
  val encrypted: Boolean,
  @SerialName("unsafePath") val unsafePath: Boolean,
  @SerialName("limitExceeded") val limitExceeded: Boolean,
)

@Serializable
internal data class RecoveryCandidateRequest(
  val version: Int,
  val source: RecoverySourceKeyInput,
  val fingerprint: RecoveryFingerprintInput,
  @SerialName("displayName") val displayName: String,
  @SerialName("spiderInfo") val spiderInfo: ByteArray? = null,
  @SerialName("comicInfo") val comicInfo: ByteArray? = null,
  val entries: List<RecoveryEntryInput> = emptyList(),
  val archive: RecoveryArchiveInput? = null,
)

@Serializable
internal enum class RecoveryEvidenceKind {
  @SerialName("spiderInfo") SPIDER_INFO,
  @SerialName("comicInfo") COMIC_INFO,
  @SerialName("looseImages") LOOSE_IMAGES,
  @SerialName("archive") ARCHIVE,
}

@Serializable
internal data class RecoveryEvidence(
  val kind: RecoveryEvidenceKind,
  val gid: Long? = null,
  val token: String? = null,
  val title: String? = null,
  @SerialName("declaredPages") val declaredPages: Int? = null,
  @SerialName("observedPages") val observedPages: Int? = null,
)

@Serializable
internal enum class RecoveryIssue {
  @SerialName("missingGid") MISSING_GID,
  @SerialName("missingToken") MISSING_TOKEN,
  @SerialName("invalidMetadata") INVALID_METADATA,
  @SerialName("identityConflict") IDENTITY_CONFLICT,
  @SerialName("pageCountConflict") PAGE_COUNT_CONFLICT,
  @SerialName("unknownPageCount") UNKNOWN_PAGE_COUNT,
  @SerialName("missingPage") MISSING_PAGE,
  @SerialName("duplicatePage") DUPLICATE_PAGE,
  @SerialName("temporaryFile") TEMPORARY_FILE,
  @SerialName("invalidImage") INVALID_IMAGE,
  @SerialName("encryptedArchive") ENCRYPTED_ARCHIVE,
  @SerialName("unreadableArchive") UNREADABLE_ARCHIVE,
  @SerialName("unsafeArchivePath") UNSAFE_ARCHIVE_PATH,
  @SerialName("limitExceeded") LIMIT_EXCEEDED,
  @SerialName("duplicateSource") DUPLICATE_SOURCE,
  @SerialName("existingMappingConflict") EXISTING_MAPPING_CONFLICT,
  @SerialName("sourceMissing") SOURCE_MISSING,
}

@Serializable
internal enum class RecoveryStatus {
  @SerialName("ready") READY,
  @SerialName("missingIdentity") MISSING_IDENTITY,
  @SerialName("incomplete") INCOMPLETE,
  @SerialName("conflict") CONFLICT,
  @SerialName("unreadable") UNREADABLE,
  @SerialName("limitExceeded") LIMIT_EXCEEDED,
  @SerialName("sourceMissing") SOURCE_MISSING,
}

@Serializable
internal enum class RecoveryPersistenceAction {
  @SerialName("promoteFinished") PROMOTE_FINISHED,
  @SerialName("stage") STAGE,
}

@Serializable
internal data class RecoveryCandidatePlan(
  @SerialName("sourceKey") val sourceKey: String,
  val fingerprint: String,
  @SerialName("sourceKind") val sourceKind: RecoverySourceKind,
  @SerialName("displayName") val displayName: String,
  val gid: Long? = null,
  val token: String? = null,
  val title: String? = null,
  @SerialName("declaredPages") val declaredPages: Int? = null,
  @SerialName("localPageCount") val localPageCount: Int,
  val evidence: List<RecoveryEvidence>,
  val issues: List<RecoveryIssue>,
  val status: RecoveryStatus,
  val action: RecoveryPersistenceAction,
)

@Serializable
internal data class ExistingRecoveryMapping(
  val gid: Long,
  @SerialName("sourceKey") val sourceKey: String,
  val complete: Boolean,
)

@Serializable
internal data class RecoveryConflictRequest(
  val version: Int,
  val candidates: List<RecoveryCandidatePlan>,
  val existing: List<ExistingRecoveryMapping>,
)

@Serializable
internal data class RecoveryConflictDecision(
  @SerialName("sourceKey") val sourceKey: String,
  val status: RecoveryStatus,
  val action: RecoveryPersistenceAction,
  val issue: RecoveryIssue? = null,
)

@Serializable
internal data class RecoveryConflictPlan(val decisions: List<RecoveryConflictDecision>)

internal data class RecoveryPageTokenEvidence(val pageIndex: Int, val pageToken: String)

internal fun extractDownloadRecoveryPageToken(spiderInfo: ByteArray): RecoveryPageTokenEvidence? =
  nativeExtractDownloadRecoveryPageTokenV1(spiderInfo)?.let { encoded ->
    val separator = encoded.indexOf(':')
    require(separator > 0) { "Rust Download Recovery returned invalid page-token evidence" }
    val pageIndex = encoded.substring(0, separator).toInt()
    val pageToken = encoded.substring(separator + 1)
    require(pageIndex >= 0 && pageToken.matches(Regex("[0-9A-Fa-f]{10}"))) {
      "Rust Download Recovery returned invalid page-token evidence"
    }
    RecoveryPageTokenEvidence(pageIndex, pageToken)
  }

internal fun inspectDownloadRecoveryCandidate(
  request: RecoveryCandidateRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeInspectDownloadRecoveryCandidateV1,
): RecoveryCandidatePlan =
  decodeRecoveryEnvelope<RecoveryCandidatePlan>(
      request = rustCbor.encodeToByteArray(request),
      operation = "Download Recovery Candidate",
      nativeCall = nativeCall,
    )
    .also(::validateCandidatePlan)

internal fun planDownloadRecoveryConflicts(
  request: RecoveryConflictRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanDownloadRecoveryConflictsV1,
): RecoveryConflictPlan {
  val plan =
    decodeRecoveryEnvelope<RecoveryConflictPlan>(
      request = rustCbor.encodeToByteArray(request),
      operation = "Download Recovery Conflicts",
      nativeCall = nativeCall,
    )
  require(plan.decisions.size == request.candidates.size) {
    "Rust Download Recovery returned an incomplete conflict plan"
  }
  val expected = request.candidates.map(RecoveryCandidatePlan::sourceKey).toSet()
  val actual = plan.decisions.map(RecoveryConflictDecision::sourceKey)
  require(actual.toSet() == expected && actual.size == actual.toSet().size) {
    "Rust Download Recovery returned invalid conflict sources"
  }
  plan.decisions.forEach { decision ->
    require(decision.sourceKey.matches(SHA_256_HEX)) {
      "Rust Download Recovery returned an invalid conflict source key"
    }
    require(
      decision.action != RecoveryPersistenceAction.PROMOTE_FINISHED ||
        decision.status == RecoveryStatus.READY
    ) {
      "Rust Download Recovery returned an unsafe promotion decision"
    }
  }
  return plan
}

private inline fun <reified T> decodeRecoveryEnvelope(
  request: ByteArray,
  operation: String,
  noinline nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): T {
  val envelope = invokeRustCbor<ApiEnvelope<T>>(request, operation, nativeCall)
  require(envelope.version == DOWNLOAD_RECOVERY_API_VERSION) {
    "Unsupported Download Recovery API version"
  }
  require(envelope.module == DOWNLOAD_RECOVERY_MODULE) {
    "Unexpected Rust module: ${envelope.module}"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Missing Download Recovery payload" }
}

private fun validateCandidatePlan(plan: RecoveryCandidatePlan) {
  require(plan.sourceKey.matches(SHA_256_HEX) && plan.fingerprint.matches(SHA_256_HEX)) {
    "Rust Download Recovery returned an invalid source identity"
  }
  require(plan.gid == null || plan.gid > 0) {
    "Rust Download Recovery returned an invalid gallery identifier"
  }
  require(plan.declaredPages == null || plan.declaredPages > 0) {
    "Rust Download Recovery returned an invalid declared page count"
  }
  require(plan.localPageCount >= 0) {
    "Rust Download Recovery returned an invalid local page count"
  }
  require(
    plan.action != RecoveryPersistenceAction.PROMOTE_FINISHED ||
      (plan.status == RecoveryStatus.READY && plan.gid != null && plan.token != null)
  ) {
    "Rust Download Recovery returned an unsafe promotion plan"
  }
}

private external fun nativeInspectDownloadRecoveryCandidateV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

private external fun nativePlanDownloadRecoveryConflictsV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

private external fun nativeExtractDownloadRecoveryPageTokenV1(spiderInfo: ByteArray): String?
