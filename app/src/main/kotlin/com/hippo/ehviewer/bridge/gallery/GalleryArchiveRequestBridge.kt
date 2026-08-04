package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Archive request operation owned by the Rust V1 planner. */
@Serializable
internal enum class GalleryArchiveRequestKind {
  @SerialName("list") LIST,
  @SerialName("download") DOWNLOAD,
}

/** Versioned request input for pure archive request planning. */
@Serializable
internal data class GalleryArchiveRequest(
  val version: Int,
  val site: GalleryUrlSite,
  val kind: GalleryArchiveRequestKind,
  val gid: Long,
  val token: String,
  val resolution: String? = null,
  @SerialName("isHath") val isHath: Boolean? = null,
) {
  init {
    val hasValidShape =
      when (kind) {
        GalleryArchiveRequestKind.LIST -> resolution == null && isHath == null
        GalleryArchiveRequestKind.DOWNLOAD -> resolution != null && isHath != null
      }
    require(hasValidShape) { "Invalid Gallery Archive request optional-field shape" }
  }
}

/** Complete archive request plan selected before HTTP starts. */
@Serializable
internal data class GalleryArchiveRequestPlan(
  val kind: GalleryArchiveRequestKind,
  val url: String,
  val referer: String,
  val origin: String?,
  val form: List<GalleryFormField>,
)

/** Calls Rust once and validates the selected request kind. */
internal fun planGalleryArchiveRequest(
  request: GalleryArchiveRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanGalleryArchiveRequestV1,
): GalleryArchiveRequestPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<GalleryArchiveRequestPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery Archive request",
      nativeCall = nativeCall,
    )
  return envelope.toGalleryArchiveRequestPlan(request.kind)
}

/** Validates one atomic Rust response. */
internal fun ApiEnvelope<GalleryArchiveRequestPlan>.toGalleryArchiveRequestPlan(
  expectedKind: GalleryArchiveRequestKind
): GalleryArchiveRequestPlan {
  require(version == GALLERY_ARCHIVE_REQUEST_API_VERSION) {
    "Unsupported Gallery Archive request version"
  }
  require(module == GALLERY_ARCHIVE_REQUEST_MODULE) {
    "Unexpected Gallery Archive request module"
  }
  error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(payload) { "Missing Gallery Archive request payload" }
  require(plan.kind == expectedKind) { "Gallery Archive request kind mismatch" }
  return plan
}

/** Selects exactly one planner before HTTP starts. */
internal fun selectGalleryArchiveRequestPlan(
  legacyPlanner: () -> GalleryArchiveRequestPlan,
  rustEnabled: () -> Boolean,
  rustPlanner: () -> GalleryArchiveRequestPlan,
): GalleryArchiveRequestPlan {
  val useRust = rustEnabled()
  return if (useRust) rustPlanner() else legacyPlanner()
}

/** Invokes the Rust V1 owner with separate direct request and response buffers. */
private external fun nativePlanGalleryArchiveRequestV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val GALLERY_ARCHIVE_REQUEST_API_VERSION = 1
internal const val GALLERY_ARCHIVE_REQUEST_MODULE = "galleryArchiveRequest"
