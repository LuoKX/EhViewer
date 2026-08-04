package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.GalleryComment as AndroidGalleryComment
import com.ehviewer.core.model.GalleryCommentList
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryCommentsPage
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates and adapts the V1 gallery-comments response to the existing Android model. */
internal fun ApiEnvelope<GalleryCommentsPage>.toGalleryCommentList(): GalleryCommentList {
  require(version == GALLERY_COMMENTS_API_VERSION) {
    "Unsupported Gallery Comments API version: $version"
  }
  require(module == GALLERY_COMMENTS_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Gallery Comments response has no payload" }
  return page.toGalleryCommentList()
}

/** Validates and maps a nested Gallery Comments payload. */
internal fun GalleryCommentsPage.toGalleryCommentList(): GalleryCommentList {
  return GalleryCommentList(
    comments =
      comments.map { comment ->
        require(comment.id >= 0) { "Gallery Comment identifier must not be negative" }
        require(comment.postedAt > 0) { "Gallery Comment posting time must be positive" }
        require(comment.lastEditedAt >= 0) { "Gallery Comment edit time must not be negative" }
        require(comment.uploader == (comment.id == 0L)) {
          "Gallery Comment uploader flag does not match its identifier"
        }
        require(!comment.votedUp || comment.canVoteUp) {
          "Gallery Comment cannot be voted up without an up-vote action"
        }
        require(!comment.votedDown || comment.canVoteDown) {
          "Gallery Comment cannot be voted down without a down-vote action"
        }
        AndroidGalleryComment(
          id = comment.id,
          score = comment.score,
          editable = comment.editable,
          voteUpAble = comment.canVoteUp,
          voteUpEd = comment.votedUp,
          voteDownAble = comment.canVoteDown,
          voteDownEd = comment.votedDown,
          uploader = comment.uploader,
          voteState = comment.voteState,
          time = comment.postedAt,
          user = comment.user,
          comment = comment.bodyHtml,
          lastEdited = comment.lastEditedAt,
        )
      },
    hasMore = hasMore,
  )
}

private const val GALLERY_COMMENTS_API_VERSION = 1
private const val GALLERY_COMMENTS_MODULE = "galleryComments"
