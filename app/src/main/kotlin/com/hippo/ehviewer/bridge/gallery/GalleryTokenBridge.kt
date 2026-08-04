package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryToken
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates the V1 Gallery Token envelope without interpreting the opaque token. */
internal fun ApiEnvelope<GalleryToken>.toGalleryToken(): String {
  require(version == GALLERY_TOKEN_API_VERSION) {
    "Unsupported Gallery Token API version: $version"
  }
  require(module == GALLERY_TOKEN_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery Token response has no payload" }.token
}

private const val GALLERY_TOKEN_API_VERSION = 1
private const val GALLERY_TOKEN_MODULE = "galleryToken"
