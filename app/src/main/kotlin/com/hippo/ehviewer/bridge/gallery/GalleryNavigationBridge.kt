package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Closed gallery navigation grammars accepted by `eh_net`. */
@Serializable
internal enum class GalleryNavigationTarget {
  /** Gallery detail or multi-page-viewer URL. */
  @SerialName("detail") DETAIL,

  /** Single reader-page URL. */
  @SerialName("page") PAGE,
}

/** Closed result kinds returned by `eh_net::gallery::navigation`. */
@Serializable
internal enum class GalleryNavigationKind {
  /** The selected grammar did not match. */
  @SerialName("none") NONE,

  /** A detail or multi-page-viewer target matched. */
  @SerialName("detail") DETAIL,

  /** A single reader-page target matched. */
  @SerialName("page") PAGE,
}

/**
 * Versioned request consumed by `eh_net::gallery::navigation`.
 *
 * @property version Protocol version, currently [GALLERY_NAVIGATION_API_VERSION].
 * @property target Detail or single-page grammar selected by the compatibility wrapper.
 * @property input Raw candidate URL or loose navigation text; it must never be logged.
 * @property strict Whether Rust must require the legacy scheme, host, and path prefix.
 */
@Serializable
internal data class GalleryNavigationRequest(
  val version: Int,
  val target: GalleryNavigationTarget,
  val input: String,
  val strict: Boolean,
)

/**
 * Parsed gallery-navigation payload with fields determined by [kind].
 *
 * @property kind No match, gallery detail, or reader page.
 * @property gid Non-negative gallery identifier for detail/page matches.
 * @property token Ten-character lowercase hexadecimal gallery or page token.
 * @property pageIndex Zero-based reader page index, present only for [GalleryNavigationKind.PAGE].
 */
@Serializable
internal data class GalleryNavigationPlan(
  val kind: GalleryNavigationKind,
  val gid: Long? = null,
  val token: String? = null,
  val pageIndex: Int? = null,
) {
  init {
    val noFields = gid == null && token == null && pageIndex == null
    val detailFields = gid != null && token != null && pageIndex == null
    val pageFields = gid != null && token != null && pageIndex != null
    require(
      when (kind) {
        GalleryNavigationKind.NONE -> noFields
        GalleryNavigationKind.DETAIL -> detailFields
        GalleryNavigationKind.PAGE -> pageFields
      }
    ) {
      "Gallery Navigation fields do not match $kind"
    }
    gid?.let { require(it >= 0) { "Gallery Navigation gid must not be negative" } }
    token?.let {
      require(it.length == TOKEN_LENGTH && it.all { character -> character in TOKEN_CHARACTERS }) {
        "Gallery Navigation token has an invalid shape"
      }
    }
    pageIndex?.let {
      require(it >= -1) { "Gallery Navigation page index must not be less than -1" }
    }
  }

  companion object {
    /** Creates the exact no-match wire shape. */
    fun none() = GalleryNavigationPlan(GalleryNavigationKind.NONE)

    /** Creates a detail match. */
    fun detail(gid: Long, token: String) =
      GalleryNavigationPlan(GalleryNavigationKind.DETAIL, gid, token)

    /** Creates a single-page match. */
    fun page(gid: Long, token: String, pageIndex: Int) =
      GalleryNavigationPlan(GalleryNavigationKind.PAGE, gid, token, pageIndex)
  }
}

/** Invokes the V1 Rust parser and validates its target-specific response shape. */
internal fun parseGalleryNavigation(
  request: GalleryNavigationRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeParseGalleryNavigationV1,
): GalleryNavigationPlan {
  val encoded = rustCbor.encodeToByteArray(request)
  return invokeRustCbor<ApiEnvelope<GalleryNavigationPlan>>(
      encoded,
      operation = "Gallery Navigation",
      nativeCall = nativeCall,
    )
    .toGalleryNavigationPlan(request.target)
}

/** Rejects an invalid envelope or target before navigation consumes the plan. */
internal fun ApiEnvelope<GalleryNavigationPlan>.toGalleryNavigationPlan(
  target: GalleryNavigationTarget
): GalleryNavigationPlan {
  require(version == GALLERY_NAVIGATION_API_VERSION) {
    "Unsupported Gallery Navigation API version: $version"
  }
  require(module == GALLERY_NAVIGATION_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(payload) { "Rust Gallery Navigation response has no payload" }
  require(
    when (target) {
      GalleryNavigationTarget.DETAIL ->
        plan.kind == GalleryNavigationKind.NONE || plan.kind == GalleryNavigationKind.DETAIL
      GalleryNavigationTarget.PAGE ->
        plan.kind == GalleryNavigationKind.NONE || plan.kind == GalleryNavigationKind.PAGE
    }
  ) {
    "Rust Gallery Navigation result does not match the requested target"
  }
  return plan
}

/** Invokes the V1 `eh_net::gallery::navigation` parser using separate direct buffers. */
private external fun nativeParseGalleryNavigationV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val GALLERY_NAVIGATION_API_VERSION = 1
internal const val GALLERY_NAVIGATION_MODULE = "galleryNavigation"
private const val TOKEN_LENGTH = 10
private val TOKEN_CHARACTERS = '0'..'9' union 'a'..'f'
