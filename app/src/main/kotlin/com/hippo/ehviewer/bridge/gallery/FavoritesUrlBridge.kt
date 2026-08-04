package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/**
 * Complete sanitized V1 state consumed by `eh_net::gallery::favorites_url`.
 *
 * @property version Protocol version, currently [FAVORITES_URL_API_VERSION].
 * @property site Public or restricted gallery host selected by Android.
 * @property favoriteCategory Legacy category: -1 is all, 0..9 is a remote slot, and every other
 *   value omits `favcat`.
 * @property keyword Optional favorites search text, omitted by Rust when blank.
 * @property previous Previous-page cursor, omitted by Rust when blank.
 * @property next Next-page cursor, omitted by Rust when blank.
 * @property seek Date cursor, omitted by Rust when blank.
 */
@Serializable
internal data class FavoritesUrlRequest(
  val version: Int,
  val site: GalleryUrlSite,
  val favoriteCategory: Int,
  val keyword: String?,
  val previous: String?,
  val next: String?,
  val seek: String?,
)

/** Invokes the V1 Rust Favorites URL builder. */
internal fun buildFavoritesUrl(
  request: FavoritesUrlRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeBuildFavoritesUrlV1,
): String {
  val encoded = rustCbor.encodeToByteArray(request)
  return invokeRustCbor<ApiEnvelope<GalleryUrlPlan>>(
      encoded,
      operation = "Favorites URL",
      nativeCall = nativeCall,
    )
    .toFavoritesUrl()
}

/** Rejects an invalid V1 envelope or unusable target before the caller uses it. */
internal fun ApiEnvelope<GalleryUrlPlan>.toFavoritesUrl(): String {
  require(version == FAVORITES_URL_API_VERSION) {
    "Unsupported Favorites URL API version: $version"
  }
  require(module == FAVORITES_URL_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  val url = checkNotNull(payload) { "Rust Favorites URL response has no payload" }.url
  require(url.isNotBlank()) { "Rust Favorites URL response is blank" }
  return url
}

/** Invokes the V1 `eh_net::gallery::favorites_url` builder using separate direct buffers. */
private external fun nativeBuildFavoritesUrlV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val FAVORITES_URL_API_VERSION = 1
internal const val FAVORITES_URL_MODULE = "galleryFavoritesUrl"
