/*
 * Copyright 2016 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryToken
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toGalleryToken
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException

object GalleryTokenApiParser {
  /** Parses a V1 response through `eh_net::gallery::token`. */
  fun parseV1(body: ByteBuffer): String = runCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryToken>>(body, ::nativeParseGalleryTokenV1)
      .toGalleryToken()
  }
    .getOrElse {
      if (it is CancellationException) throw it
      if (it is RustFailureException) throw it
      throw ParseException("Parse V1 Gallery Token response error", it)
    }

  /** Invokes the V1 Rust response parser while request secrets remain owned by Android. */
  private external fun nativeParseGalleryTokenV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
