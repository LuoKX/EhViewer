package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Coarse Gallery networking operation owned by `eh_net::gallery::network_policy`. */
@Serializable
internal enum class GalleryNetworkPolicyKind {
  @SerialName("metadata") METADATA,
  @SerialName("archiveDownload") ARCHIVE_DOWNLOAD,
}

/** Versioned input for one whole Gallery networking policy operation. */
@Serializable
internal data class GalleryNetworkPolicyRequest(
  val version: Int,
  val kind: GalleryNetworkPolicyKind,
  val itemCount: Int?,
  @SerialName("isHath") val isHath: Boolean?,
) {
  init {
    val valid =
      when (kind) {
        GalleryNetworkPolicyKind.METADATA -> itemCount != null && itemCount >= 0 && isHath == null
        GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD -> itemCount == null && isHath != null
      }
    require(valid) { "Invalid Gallery network policy request shape" }
  }

  companion object {
    /** Builds one whole-list Gallery Metadata policy request. */
    fun metadata(itemCount: Int) =
      GalleryNetworkPolicyRequest(
        version = GALLERY_NETWORK_POLICY_API_VERSION,
        kind = GalleryNetworkPolicyKind.METADATA,
        itemCount = itemCount,
        isHath = null,
      )

    /** Builds one whole Gallery Archive download policy request. */
    fun archiveDownload(isHath: Boolean) =
      GalleryNetworkPolicyRequest(
        version = GALLERY_NETWORK_POLICY_API_VERSION,
        kind = GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD,
        itemCount = null,
        isHath = isHath,
      )
  }
}

/** Complete coarse policy response; exactly one operation payload is present. */
@Serializable
internal data class GalleryNetworkPolicyPlan(
  val kind: GalleryNetworkPolicyKind,
  val metadata: GalleryMetadataPolicyPlan?,
  val archiveDownload: GalleryArchiveDownloadPolicyPlan?,
)

/** Whole-list `gdata` batching and wave-rate-limit plan. */
@Serializable
internal data class GalleryMetadataPolicyPlan(
  val itemCount: Int,
  val itemsPerRequest: Int,
  val requestsPerWave: Int,
  val requestCount: Int,
  val waveCount: Int,
  val waveDelayMillis: Long,
)

/** One archive attempt and its Android-owned pre-attempt delay. */
@Serializable internal data class GalleryArchiveAttemptPlan(val delayBeforeMillis: Long)

/** Outcome when the archive parser yields a non-null URL. */
@Serializable
internal enum class GalleryArchiveParsedOutcome {
  @SerialName("returnUrl") RETURN_URL,
  @SerialName("returnNull") RETURN_NULL,
}

/** Outcome after every planned archive attempt parses as null. */
@Serializable
internal enum class GalleryArchiveNullOutcome {
  @SerialName("returnNull") RETURN_NULL,
  @SerialName("archiveUnavailable") ARCHIVE_UNAVAILABLE,
}

/** Whole Gallery Archive attempt and terminal-outcome plan. */
@Serializable
internal data class GalleryArchiveDownloadPolicyPlan(
  @SerialName("isHath") val isHath: Boolean,
  val attempts: List<GalleryArchiveAttemptPlan>,
  val parsedOutcome: GalleryArchiveParsedOutcome,
  val exhaustedNullOutcome: GalleryArchiveNullOutcome,
)

/** Injectable signature of the one coarse Gallery network-policy JNI operation. */
internal typealias GalleryNetworkPolicyNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

/** Plans and validates batching for one complete Gallery Metadata list before HTTP. */
internal fun planGalleryMetadataPolicy(
  itemCount: Int,
  nativeCall: GalleryNetworkPolicyNativeCall = ::nativePlanGalleryNetworkPolicyV1,
): GalleryMetadataPolicyPlan {
  val request = GalleryNetworkPolicyRequest.metadata(itemCount)
  return planGalleryNetworkPolicy(request, nativeCall).metadata
    ?: error("Missing Gallery Metadata policy")
}

/** Plans and validates all Gallery Archive attempts before the first HTTP request. */
internal fun planGalleryArchiveDownloadPolicy(
  isHath: Boolean,
  nativeCall: GalleryNetworkPolicyNativeCall = ::nativePlanGalleryNetworkPolicyV1,
): GalleryArchiveDownloadPolicyPlan {
  val request = GalleryNetworkPolicyRequest.archiveDownload(isHath)
  return planGalleryNetworkPolicy(request, nativeCall).archiveDownload
    ?: error("Missing Gallery Archive download policy")
}

/** Partitions caller-owned items according to one validated Rust metadata plan. */
internal fun <T> GalleryMetadataPolicyPlan.toRequestWaves(items: List<T>): List<List<List<T>>> {
  require(items.size == itemCount) { "Gallery Metadata policy item count changed" }
  val requests = items.chunked(itemsPerRequest)
  check(requests.size == requestCount) { "Gallery Metadata request count drifted" }
  val waves = requests.chunked(requestsPerWave)
  check(waves.size == waveCount) { "Gallery Metadata wave count drifted" }
  return waves
}

private fun planGalleryNetworkPolicy(
  request: GalleryNetworkPolicyRequest,
  nativeCall: GalleryNetworkPolicyNativeCall,
): GalleryNetworkPolicyPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<GalleryNetworkPolicyPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery network policy",
      nativeCall = nativeCall,
    )
  return envelope.toGalleryNetworkPolicyPlan(request)
}

/** Validates one atomic Rust policy envelope before Android starts any effect. */
internal fun ApiEnvelope<GalleryNetworkPolicyPlan>.toGalleryNetworkPolicyPlan(
  request: GalleryNetworkPolicyRequest
): GalleryNetworkPolicyPlan {
  require(version == GALLERY_NETWORK_POLICY_API_VERSION) {
    "Unsupported Gallery network policy version"
  }
  require(module == GALLERY_NETWORK_POLICY_MODULE) {
    "Unexpected Gallery network policy module"
  }
  error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(payload) { "Missing Gallery network policy payload" }
  require(plan.kind == request.kind) { "Gallery network policy kind mismatch" }
  when (request.kind) {
    GalleryNetworkPolicyKind.METADATA -> {
      require(plan.archiveDownload == null) { "Unexpected Gallery Archive policy" }
      val metadata = checkNotNull(plan.metadata) { "Missing Gallery Metadata policy" }
      validateMetadataPolicy(metadata, requireNotNull(request.itemCount))
    }
    GalleryNetworkPolicyKind.ARCHIVE_DOWNLOAD -> {
      require(plan.metadata == null) { "Unexpected Gallery Metadata policy" }
      val archive = checkNotNull(plan.archiveDownload) { "Missing Gallery Archive policy" }
      validateArchivePolicy(archive, requireNotNull(request.isHath))
    }
  }
  return plan
}

private fun validateMetadataPolicy(plan: GalleryMetadataPolicyPlan, itemCount: Int) {
  require(plan.itemCount == itemCount) { "Gallery Metadata policy item count mismatch" }
  require(plan.itemsPerRequest > 0) { "Gallery Metadata request size must be positive" }
  require(plan.requestsPerWave > 0) { "Gallery Metadata wave size must be positive" }
  require(plan.requestCount >= 0) { "Gallery Metadata request count must not be negative" }
  require(plan.waveCount >= 0) { "Gallery Metadata wave count must not be negative" }
  require(plan.waveDelayMillis >= 0) { "Gallery Metadata wave delay must not be negative" }
}

private fun validateArchivePolicy(plan: GalleryArchiveDownloadPolicyPlan, isHath: Boolean) {
  require(plan.isHath == isHath) { "Gallery Archive policy request mismatch" }
  require(plan.attempts.isNotEmpty()) { "Gallery Archive policy has no attempts" }
  require(plan.attempts.size <= 2) { "Gallery Archive policy exceeds its attempt budget" }
  require(plan.attempts.first().delayBeforeMillis == 0L) {
    "Gallery Archive first attempt must not be delayed"
  }
  require(plan.attempts.all { it.delayBeforeMillis >= 0 }) {
    "Gallery Archive attempt delay must not be negative"
  }
}

/**
 * Invokes the single V1 Gallery network-policy boundary.
 *
 * Both buffers are direct and caller-owned. Rust reads exactly [requestLength] bytes, writes one
 * atomic CBOR envelope, retains no references, and performs no HTTP or delay effect.
 */
private external fun nativePlanGalleryNetworkPolicyV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

internal const val GALLERY_NETWORK_POLICY_API_VERSION = 1
internal const val GALLERY_NETWORK_POLICY_MODULE = "galleryNetworkPolicy"
internal const val GALLERY_ARCHIVE_UNAVAILABLE_MESSAGE = "Archive unavailable"
