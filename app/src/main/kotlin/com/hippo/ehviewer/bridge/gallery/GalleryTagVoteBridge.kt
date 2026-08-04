package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.GalleryTagGroup as AndroidGalleryTagGroup
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryTags
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates and adapts the V1 Gallery Tag Vote response to Android tag groups. */
internal fun ApiEnvelope<GalleryTags>.toGalleryTagVoteResult(): List<AndroidGalleryTagGroup> {
  require(version == GALLERY_TAG_VOTE_API_VERSION) {
    "Unsupported Gallery Tag Vote API version: $version"
  }
  require(module == GALLERY_TAG_VOTE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery Tag Vote response has no payload" }
    .groups
    .toAndroidGalleryTagGroups(
      operation = "Gallery Tag Vote",
      requireNonBlankText = false,
    )
}

private const val GALLERY_TAG_VOTE_API_VERSION = 1
private const val GALLERY_TAG_VOTE_MODULE = "galleryTagVote"
