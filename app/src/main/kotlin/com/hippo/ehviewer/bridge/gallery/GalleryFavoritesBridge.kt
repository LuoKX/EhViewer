package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryFavoritesPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.FavParserResult

/** Validates and adapts a V1 Gallery Favorites response to the Android parser model. */
internal fun ApiEnvelope<GalleryFavoritesPage>.toFavoritesResult(): FavParserResult {
  require(version == GALLERY_FAVORITES_API_VERSION) {
    "Unsupported Gallery Favorites API version: $version"
  }
  require(module == GALLERY_FAVORITES_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  val result = checkNotNull(payload) { "Rust Gallery Favorites response has no payload" }
  require(result.categories.size == REMOTE_FAVORITE_CATEGORY_COUNT) {
    "Rust Gallery Favorites response must contain ten category names"
  }
  require(result.counts.size == REMOTE_FAVORITE_CATEGORY_COUNT) {
    "Rust Gallery Favorites response must contain ten category counts"
  }
  return FavParserResult(
    catArray = result.categories,
    countArray = result.counts,
    galleryListResult = result.page.toGalleryListResult(),
  )
}

private const val GALLERY_FAVORITES_API_VERSION = 1
private const val GALLERY_FAVORITES_MODULE = "galleryFavorites"
private const val REMOTE_FAVORITE_CATEGORY_COUNT = 10
