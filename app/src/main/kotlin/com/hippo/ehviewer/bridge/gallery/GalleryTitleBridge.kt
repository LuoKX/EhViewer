package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Versioned sanitized input for one similar-gallery title cleanup. */
@Serializable
internal data class GalleryTitleRequest(
  val version: Int,
  val input: String?,
)

/** Complete pure result of one similar-gallery title cleanup. */
@Serializable internal data class GalleryTitlePlan(val title: String?)

/** Calls the Rust title owner once and returns its validated search title. */
internal fun extractGalleryTitle(
  input: String?,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanGalleryTitleV1,
): String? {
  val envelope =
    invokeRustCbor<ApiEnvelope<GalleryTitlePlan>>(
      rustCbor.encodeToByteArray(GalleryTitleRequest(GALLERY_TITLE_API_VERSION, input)),
      operation = "Gallery title",
      nativeCall = nativeCall,
    )
  require(envelope.version == GALLERY_TITLE_API_VERSION) {
    "Unsupported Gallery title API version"
  }
  require(envelope.module == GALLERY_TITLE_MODULE) { "Unexpected Gallery title module" }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Missing Gallery title payload" }.title
}

/** Invokes the V1 `eh_domain::gallery_rules` title planner. */
private external fun nativePlanGalleryTitleV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val GALLERY_TITLE_API_VERSION = 1
internal const val GALLERY_TITLE_MODULE = "galleryTitle"
