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

import arrow.core.Either
import arrow.core.getOrElse
import com.ehviewer.core.util.unescapeXml
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ReaderPage as RustReaderPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.reader.toGalleryPageResult
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer

object GalleryPageParser {
  private val PATTERN_IMAGE_URL = Regex("<img[^>]*src=\"([^\"]+)\" style")
  private val PATTERN_SKIP_HATH_KEY = Regex("onclick=\"return nl\\('([^)]+)'\\)")
  private val PATTERN_ORIGIN_IMAGE_URL = Regex("<a href=\"([^\"]+/fullimg/[^\"]+)\">")

  // TODO Not sure about the size of show keys
  private val PATTERN_SHOW_KEY = Regex("var showkey=\"([0-9a-z]+)\";")

  fun parse(body: String): Result {
    val imageUrl =
      PATTERN_IMAGE_URL.find(body)?.run {
        groupValues[1].unescapeXml()
      }
    val skipHathKey =
      PATTERN_SKIP_HATH_KEY.find(body)?.run {
        groupValues[1]
      }
    val originImageUrl =
      PATTERN_ORIGIN_IMAGE_URL.find(body)?.run {
        groupValues[1].unescapeXml()
      }
    val showKey =
      PATTERN_SHOW_KEY.find(body)?.run {
        groupValues[1]
      }
    return if (!imageUrl.isNullOrEmpty()) {
      Result(imageUrl, skipHathKey, originImageUrl, showKey)
    } else {
      throw ParseException("Parse image url error")
    }
  }

  fun parseV1(body: ByteBuffer): Result =
    Either.catch {
        unmarshalParsingAs<ApiEnvelope<RustReaderPage>>(body, ::nativeParseReaderPageV1)
          .toGalleryPageResult()
      }
      .getOrElse {
        if (it is RustFailureException) throw it
        throw ParseException("Parse reader page error", it)
      }

  class Result(
    val imageUrl: String,
    val skipHathKey: String?,
    val originImageUrl: String?,
    val showKey: String?,
  )

  /** Invokes the V1 `eh_net::reader::page` parser. */
  private external fun nativeParseReaderPageV1(body: ByteBuffer, size: Int = body.limit()): Int
}
