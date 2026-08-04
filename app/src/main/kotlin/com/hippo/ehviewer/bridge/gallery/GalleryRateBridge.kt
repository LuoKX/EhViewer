package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryRating
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.RateGalleryResult

/** Validates and adapts the V1 Gallery Rate response to the existing Android result. */
internal fun ApiEnvelope<GalleryRating>.toRateGalleryResult(): RateGalleryResult {
  require(version == GALLERY_RATE_API_VERSION) {
    "Unsupported Gallery Rate API version: $version"
  }
  require(module == GALLERY_RATE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery Rate response has no payload" }
    .let {
      RateGalleryResult(rating = it.rating, ratingCount = it.ratingCount)
    }
}

private const val GALLERY_RATE_API_VERSION = 1
private const val GALLERY_RATE_MODULE = "galleryRate"
