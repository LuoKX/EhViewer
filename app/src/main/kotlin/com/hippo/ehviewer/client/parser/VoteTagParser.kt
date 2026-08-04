/*
 * Copyright 2022 Tarsin Norbin
 *
 * This file is part of EhViewer
 *
 * EhViewer is free software: you can redistribute it and/or
 * modify it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * EhViewer is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
 * or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with EhViewer.
 * If not, see <https://www.gnu.org/licenses/>.
 */
package com.hippo.ehviewer.client.parser

import arrow.core.Either.Companion.catch
import arrow.core.getOrElse
import com.ehviewer.core.model.GalleryTagGroup
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryTags
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toGalleryTagVoteResult
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException

object VoteTagParser {
  /** Parses the unversioned response retained as the default-off rollback path. */
  fun parse(body: ByteBuffer): List<GalleryTagGroup> = catch {
    unmarshalParsingAs<List<GalleryTagGroup>>(body, ::nativeParse)
  }
    .getOrElse {
      if (it is CancellationException) throw it
      throw ParseException("Failed to parse vote tag result", it)
    }

  /** Parses a V1 response through `eh_net::gallery::tag_vote`. */
  fun parseV1(body: ByteBuffer): List<GalleryTagGroup> = runCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryTags>>(body, ::nativeParseTagVoteV1)
      .toGalleryTagVoteResult()
  }
    .getOrElse {
      if (it is CancellationException) throw it
      if (it is RustFailureException) throw it
      throw ParseException("Failed to parse V1 vote tag result", it)
    }

  /** Selects exactly one parser before the HTTP response body is consumed. */
  internal suspend fun parseWithAdapters(
    useRust: Boolean,
    legacyParser: suspend () -> List<GalleryTagGroup>,
    rustParser: suspend () -> List<GalleryTagGroup>,
  ): List<GalleryTagGroup> = if (useRust) rustParser() else legacyParser()

  private external fun nativeParse(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes V1 parsing while credentials and submitted tag data remain on Android. */
  private external fun nativeParseTagVoteV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
