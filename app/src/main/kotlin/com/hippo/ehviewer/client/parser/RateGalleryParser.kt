/*
 * Copyright 2026 EhViewer contributors
 *
 * This file is part of EhViewer and is distributed under the GNU General Public License v3.0.
 */
package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryRating
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toRateGalleryResult
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException

/** Parses Gallery Rate responses through the V1 Rust contract. */
object RateGalleryParser {
  /** Parses a V1 response through `eh_net::gallery::rate`. */
  fun parseV1(body: ByteBuffer): RateGalleryResult = runCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryRating>>(body, ::nativeParseGalleryRateV1)
      .toRateGalleryResult()
  }
    .getOrElse {
      if (it is CancellationException) throw it
      if (it is RustFailureException) throw it
      throw ParseException("Failed to parse V1 gallery rate result", it)
    }

  /** Invokes V1 parsing while credentials, request rounding, and HTTP stay on Android. */
  private external fun nativeParseGalleryRateV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
