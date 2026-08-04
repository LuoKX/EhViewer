package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryCommentVote
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.VoteCommentResult

/** Validates and adapts the V1 Gallery Comment Vote response to the Android DTO. */
internal fun ApiEnvelope<GalleryCommentVote>.toVoteCommentResult(): VoteCommentResult {
  require(version == GALLERY_COMMENT_VOTE_API_VERSION) {
    "Unsupported Gallery Comment Vote API version: $version"
  }
  require(module == GALLERY_COMMENT_VOTE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Gallery Comment Vote response has no payload" }
    .let {
      VoteCommentResult(id = it.id, score = it.score, vote = it.vote)
    }
}

private const val GALLERY_COMMENT_VOTE_API_VERSION = 1
private const val GALLERY_COMMENT_VOTE_MODULE = "galleryCommentVote"
