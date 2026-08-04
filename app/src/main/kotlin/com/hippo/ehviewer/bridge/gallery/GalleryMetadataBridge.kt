package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryMetadataBatch
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates the V1 Gallery List metadata envelope before Android mutates list entries. */
internal fun ApiEnvelope<GalleryMetadataBatch>.toGalleryMetadataBatch(): GalleryMetadataBatch {
  require(version == GALLERY_METADATA_API_VERSION) {
    "Unsupported Gallery Metadata API version: $version"
  }
  require(module == GALLERY_METADATA_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery Metadata response has no payload" }
}

private const val GALLERY_METADATA_API_VERSION = 1
private const val GALLERY_METADATA_MODULE = "galleryMetadata"
