package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Stable Gallery List URL modes accepted by `eh_net`. */
@Serializable
internal enum class GalleryListMode {
  @SerialName("normal") NORMAL,
  @SerialName("uploader") UPLOADER,
  @SerialName("tag") TAG,
  @SerialName("whatsHot") WHATS_HOT,
  @SerialName("imageSearch") IMAGE_SEARCH,
  @SerialName("subscription") SUBSCRIPTION,
  @SerialName("toplist") TOPLIST,
}

/**
 * Complete sanitized V1 state consumed by `eh_net::gallery::list_url`.
 *
 * @property version Protocol version, currently [GALLERY_LIST_URL_API_VERSION].
 * @property site Public or restricted gallery host selected by Android.
 * @property signedIn Whether Android currently has an authenticated gallery session.
 * @property mode Closed catalog URL mode.
 * @property previous Previous-page cursor, omitted by Rust when blank.
 * @property next Next-page cursor, omitted by Rust when blank.
 * @property seek Date/toplist cursor, omitted by Rust when blank.
 * @property range Legacy range filter, emitted only when positive.
 * @property category Android category mask before remote-protocol inversion.
 * @property keyword Search, uploader, tag, or toplist value depending on [mode].
 * @property imageHash Reverse image-search hash.
 * @property languageTag Already selected `language:*` tag, or null for no language filter.
 * @property advancedSearch Legacy advanced-search bit mask.
 * @property minimumRating Minimum rating filter, emitted only when positive.
 * @property pageFrom Minimum page-count filter, emitted only when positive.
 * @property pageTo Maximum page-count filter, emitted only when positive.
 */
@Serializable
internal data class GalleryListUrlRequest(
  val version: Int,
  val site: GalleryUrlSite,
  val signedIn: Boolean,
  val mode: GalleryListMode,
  val previous: String?,
  val next: String?,
  val seek: String?,
  val range: Int,
  val category: Int,
  val keyword: String?,
  val imageHash: String?,
  val languageTag: String?,
  val advancedSearch: Int,
  val minimumRating: Int,
  val pageFrom: Int,
  val pageTo: Int,
)

/** Invokes the V1 Rust Gallery List URL builder. */
internal fun buildGalleryListUrl(
  request: GalleryListUrlRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeBuildGalleryListUrlV1,
): String {
  val encoded = rustCbor.encodeToByteArray(request)
  return invokeRustCbor<ApiEnvelope<GalleryUrlPlan>>(
      encoded,
      operation = "Gallery List URL",
      nativeCall = nativeCall,
    )
    .toGalleryListUrl()
}

/** Rejects an invalid envelope or unusable target before the caller uses it. */
internal fun ApiEnvelope<GalleryUrlPlan>.toGalleryListUrl(): String {
  require(version == GALLERY_LIST_URL_API_VERSION) {
    "Unsupported Gallery List URL API version: $version"
  }
  require(module == GALLERY_LIST_URL_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  val url = checkNotNull(payload) { "Rust Gallery List URL response has no payload" }.url
  require(url.isNotBlank()) { "Rust Gallery List URL response is blank" }
  return url
}

/** Invokes the V1 `eh_net::gallery::list_url` builder using separate direct buffers. */
private external fun nativeBuildGalleryListUrlV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val GALLERY_LIST_URL_API_VERSION = 1
internal const val GALLERY_LIST_URL_MODULE = "galleryListUrl"
