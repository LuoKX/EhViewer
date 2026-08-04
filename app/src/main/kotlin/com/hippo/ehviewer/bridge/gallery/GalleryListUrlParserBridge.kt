package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/**
 * First decoded values of the query fields consumed by the legacy Gallery List URL parser.
 *
 * @property imageHash First `f_shash` value; presence, including an empty value, selects image
 *   search at the root.
 * @property previous First `prev` page cursor.
 * @property next First `next` page cursor.
 * @property seek First `seek` date cursor.
 * @property range First `range` text, interpreted by Rust as a signed integer or zero.
 * @property excludedCategories First `f_cats` remote exclusion mask.
 * @property search First `f_search` expression.
 * @property advancedSearch First `advsearch` marker.
 * @property searchName First `f_sh` marker.
 * @property searchTags First `f_sto` marker.
 * @property searchDescription First `f_sfl` marker.
 * @property searchTorrentFilenames First `f_sfu` marker.
 * @property searchLowPowerTags First `f_sft` marker.
 * @property minimumRating First `f_srdd` value.
 * @property pageFrom First `f_spf` value.
 * @property pageTo First `f_spt` value.
 * @property toplist First `tl` toplist identifier.
 * @property toplistPage First `p` toplist cursor.
 */
@Serializable
internal data class GalleryListUrlParameters(
  val imageHash: String?,
  val previous: String?,
  val next: String?,
  val seek: String?,
  val range: String?,
  val excludedCategories: String?,
  val search: String?,
  val advancedSearch: String?,
  val searchName: String?,
  val searchTags: String?,
  val searchDescription: String?,
  val searchTorrentFilenames: String?,
  val searchLowPowerTags: String?,
  val minimumRating: String?,
  val pageFrom: String?,
  val pageTo: String?,
  val toplist: String?,
  val toplistPage: String?,
)

/**
 * Sanitized result of Android's Ktor syntax parsing and exact-host preflight.
 *
 * @property segments At most the first two meaningful, once-decoded path segments.
 * @property parameters First decoded values for only the query fields used by Gallery List state.
 */
@Serializable
internal data class GalleryListUrlSnapshot(
  val segments: List<String>,
  val parameters: GalleryListUrlParameters,
)

/**
 * Versioned request consumed by `eh_net::gallery::list_url_parser`.
 *
 * @property version Protocol version, currently [GALLERY_LIST_URL_PARSER_API_VERSION].
 * @property snapshot Sanitized URL facts retained by Android's legacy parser.
 */
@Serializable
internal data class GalleryListUrlParseRequest(
  val version: Int,
  val snapshot: GalleryListUrlSnapshot,
)

/**
 * Complete platform-neutral state of the existing Android `ListUrlBuilder`.
 *
 * @property mode Legacy integer mode discriminant.
 * @property previous Previous-page cursor.
 * @property next Next-page cursor.
 * @property seek Date or toplist cursor.
 * @property range Parsed range, defaulting to zero.
 * @property category Android category mask, or `-1` when absent or invalid.
 * @property keyword Search, uploader, tag, or toplist value.
 * @property imageHash Image-search hash when root `f_shash` is present.
 * @property language Language index; URL parsing leaves it at `-1`.
 * @property advancedSearch Parsed advanced-search bit mask, or `-1` when disabled.
 * @property minimumRating Parsed minimum rating, or `-1` when disabled or invalid.
 * @property pageFrom Parsed minimum page count, or `-1` when disabled or invalid.
 * @property pageTo Parsed maximum page count, or `-1` when disabled or invalid.
 */
@Serializable
internal data class GalleryListUrlState(
  val mode: Int,
  val previous: String?,
  val next: String?,
  val seek: String?,
  val range: Int,
  val category: Int,
  val keyword: String?,
  val imageHash: String?,
  val language: Int,
  val advancedSearch: Int,
  val minimumRating: Int,
  val pageFrom: Int,
  val pageTo: Int,
)

/**
 * V1 parser payload.
 *
 * @property state Parsed builder state, or null when the legacy parser would reject the shape.
 */
@Serializable internal data class GalleryListUrlParsePlan(val state: GalleryListUrlState?)

/** Invokes the V1 Rust parser and returns its selected state or no-match result. */
internal fun parseGalleryListUrl(
  request: GalleryListUrlParseRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeParseGalleryListUrlV1,
): GalleryListUrlState? {
  val encoded = rustCbor.encodeToByteArray(request)
  return invokeRustCbor<ApiEnvelope<GalleryListUrlParsePlan>>(
      encoded,
      operation = "Gallery List URL Parser",
      nativeCall = nativeCall,
    )
    .toGalleryListUrlState()
}

/** Rejects an invalid envelope before the caller uses parsed navigation state. */
internal fun ApiEnvelope<GalleryListUrlParsePlan>.toGalleryListUrlState(): GalleryListUrlState? {
  require(version == GALLERY_LIST_URL_PARSER_API_VERSION) {
    "Unsupported Gallery List URL Parser API version: $version"
  }
  require(module == GALLERY_LIST_URL_PARSER_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery List URL Parser response has no payload" }.state
}

/** Invokes the V1 sanitized `eh_net::gallery::list_url_parser` operation. */
private external fun nativeParseGalleryListUrlV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val GALLERY_LIST_URL_PARSER_API_VERSION = 1
internal const val GALLERY_LIST_URL_PARSER_MODULE = "galleryListUrlParser"
