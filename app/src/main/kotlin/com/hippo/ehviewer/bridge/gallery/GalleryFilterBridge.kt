package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** One complete Gallery list or comment filtering operation. */
@Serializable
internal enum class GalleryFilterOperation {
  @SerialName("preEnrichment") PRE_ENRICHMENT,
  @SerialName("postEnrichment") POST_ENRICHMENT,
  @SerialName("comment") COMMENT,
}

/** Immutable projection of one Android-owned Room filter row. */
@Serializable
internal data class GalleryFilterSnapshot(
  val mode: Int,
  val text: String,
  val enabled: Boolean,
)

/** Portable fields needed to filter one Android-owned Gallery row. */
@Serializable
internal data class GalleryFilterGallery(
  val title: String?,
  val uploader: String?,
  val tags: List<String>?,
)

/** Portable fields needed to filter one Android-owned Gallery comment. */
@Serializable
internal data class GalleryFilterComment(
  val score: Int,
  val uploader: Boolean,
  val commenter: String?,
)

/** Versioned request for one complete filtering batch. */
@Serializable
internal data class GalleryFilterRequest(
  val version: Int,
  val operation: GalleryFilterOperation,
  val filters: List<GalleryFilterSnapshot>,
  val galleries: List<GalleryFilterGallery>,
  val comments: List<GalleryFilterComment>,
  val scoreThreshold: Int?,
  val commentContentMatches: List<List<Boolean?>>?,
) {
  init {
    val valid =
      when (operation) {
        GalleryFilterOperation.PRE_ENRICHMENT,
        GalleryFilterOperation.POST_ENRICHMENT ->
          comments.isEmpty() && scoreThreshold == null && commentContentMatches == null
        GalleryFilterOperation.COMMENT ->
          galleries.isEmpty() &&
            scoreThreshold != null &&
            (commentContentMatches == null ||
              commentContentMatches.size == comments.size &&
                commentContentMatches.all { it.size == filters.size })
      }
    require(valid) { "Invalid Gallery filter request shape" }
  }

  companion object {
    fun galleries(
      operation: GalleryFilterOperation,
      filters: List<GalleryFilterSnapshot>,
      galleries: List<GalleryFilterGallery>,
    ) =
      GalleryFilterRequest(
        version = GALLERY_FILTER_API_VERSION,
        operation = operation,
        filters = filters,
        galleries = galleries,
        comments = emptyList(),
        scoreThreshold = null,
        commentContentMatches = null,
      )

    fun comments(
      filters: List<GalleryFilterSnapshot>,
      comments: List<GalleryFilterComment>,
      scoreThreshold: Int,
      commentContentMatches: List<List<Boolean?>>? = null,
    ) =
      GalleryFilterRequest(
        version = GALLERY_FILTER_API_VERSION,
        operation = GalleryFilterOperation.COMMENT,
        filters = filters,
        galleries = emptyList(),
        comments = comments,
        scoreThreshold = scoreThreshold,
        commentContentMatches = commentContentMatches,
      )
  }
}

/** Atomic Rust keep/drop plan aligned with the selected operation's subjects. */
@Serializable
internal data class GalleryFilterPlan(
  val operation: GalleryFilterOperation,
  val keep: List<Boolean>,
  val needsTags: Boolean,
  val commentEvaluationFilterIndices: List<List<Int>>,
)

/** Injectable signature of the one batch Gallery-filter JNI operation. */
internal typealias GalleryFilterNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

/** Invokes and validates one atomic Gallery-filter plan before any list mutation. */
internal fun planGalleryFilter(
  request: GalleryFilterRequest,
  nativeCall: GalleryFilterNativeCall = ::nativePlanGalleryFilterV1,
): GalleryFilterPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<GalleryFilterPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery filter",
      nativeCall = nativeCall,
    )
  return envelope.toGalleryFilterPlan(request)
}

/** Validates the complete response envelope and aligned plan shape. */
internal fun ApiEnvelope<GalleryFilterPlan>.toGalleryFilterPlan(
  request: GalleryFilterRequest
): GalleryFilterPlan {
  require(version == GALLERY_FILTER_API_VERSION) { "Unsupported Gallery filter version" }
  require(module == GALLERY_FILTER_MODULE) { "Unexpected Gallery filter module" }
  error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(payload) { "Missing Gallery filter payload" }
  require(plan.operation == request.operation) { "Gallery filter operation mismatch" }
  when (request.operation) {
    GalleryFilterOperation.PRE_ENRICHMENT,
    GalleryFilterOperation.POST_ENRICHMENT -> {
      require(plan.keep.size == request.galleries.size) { "Gallery filter plan length mismatch" }
      require(plan.commentEvaluationFilterIndices.isEmpty()) {
        "Gallery list filter returned comment evaluation instructions"
      }
    }
    GalleryFilterOperation.COMMENT ->
      if (request.commentContentMatches == null) {
        require(plan.keep.isEmpty()) { "Gallery comment preflight returned keep decisions" }
        require(plan.commentEvaluationFilterIndices.size == request.comments.size) {
          "Gallery comment preflight length mismatch"
        }
        plan.commentEvaluationFilterIndices.forEach { indices ->
          require(indices.zipWithNext().all { (left, right) -> left < right }) {
            "Gallery comment evaluation indexes are not strictly ordered"
          }
          indices.forEach { index ->
            val filter = request.filters.getOrNull(index)
            require(filter != null && filter.enabled && filter.mode == COMMENT_FILTER_MODE) {
              "Gallery comment evaluation index does not select an enabled content filter"
            }
          }
        }
      } else {
        require(plan.keep.size == request.comments.size) { "Gallery filter plan length mismatch" }
        require(plan.commentEvaluationFilterIndices.isEmpty()) {
          "Gallery comment result returned evaluation instructions"
        }
      }
  }
  return plan
}

/**
 * Invokes the single V1 Gallery-filter boundary.
 *
 * Both buffers are direct and caller-owned. Rust reads exactly [requestLength] bytes, writes one
 * atomic CBOR envelope, retains no references, and performs no Android mutation.
 */
private external fun nativePlanGalleryFilterV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

internal const val GALLERY_FILTER_API_VERSION = 1
internal const val GALLERY_FILTER_MODULE = "galleryFilter"
private const val COMMENT_FILTER_MODE = 5
