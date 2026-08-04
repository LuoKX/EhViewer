/*
 * Copyright 2026 EhViewer contributors
 *
 * This file is part of EhViewer and is distributed under the GNU General Public License v3.0.
 */
package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryCommentVote
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toVoteCommentResult
import com.hippo.ehviewer.client.exception.ParseException
import com.hippo.ehviewer.client.parseAs
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException

/** Parses comment-vote responses through either the rollback or V1 Rust contract. */
object VoteCommentParser {
  /** Parses the legacy JSON response without adding policy beyond its serializable DTO. */
  fun parse(body: String): VoteCommentResult = body.parseAs()

  /** Parses a V1 response through `eh_net::gallery::comment_vote`. */
  fun parseV1(body: ByteBuffer): VoteCommentResult = runCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryCommentVote>>(body, ::nativeParseVoteCommentV1)
      .toVoteCommentResult()
  }
    .getOrElse {
      if (it is CancellationException) throw it
      if (it is RustFailureException) throw it
      throw ParseException("Failed to parse V1 gallery comment vote result", it)
    }

  /** Selects exactly one parser before the HTTP response body is consumed. */
  internal suspend fun parseWithAdapters(
    useRust: Boolean,
    legacyParser: suspend () -> VoteCommentResult,
    rustParser: suspend () -> VoteCommentResult,
  ): VoteCommentResult = if (useRust) rustParser() else legacyParser()

  /** Invokes V1 parsing while credentials, submitted vote, HTTP, and refresh stay on Android. */
  private external fun nativeParseVoteCommentV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
