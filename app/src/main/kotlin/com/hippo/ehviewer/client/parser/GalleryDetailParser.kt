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
import com.ehviewer.core.model.GalleryCommentList
import com.ehviewer.core.model.GalleryDetail
import com.ehviewer.core.model.GalleryPreviewList
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryCommentsPage as RustGalleryCommentsPage
import com.hippo.ehviewer.bridge.GalleryDetailPage as RustGalleryDetailPage
import com.hippo.ehviewer.bridge.GalleryPreviewPage as RustGalleryPreviewPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toGalleryCommentList
import com.hippo.ehviewer.bridge.gallery.toGalleryDetailResult
import com.hippo.ehviewer.bridge.gallery.toGalleryPreviewList
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.CborArray

object GalleryDetailParser {
  fun parse(body: ByteBuffer) =
    Either.catch {
        if (Settings.useRustCore.value) {
          unmarshalParsingAs<ApiEnvelope<RustGalleryDetailPage>>(body, ::nativeParseV1)
            .toGalleryDetailResult()
        } else {
          unmarshalParsingAs<Result>(body, ::nativeParse)
        }
      }
      .getOrElse {
        if (it is RustFailureException) throw it
        throw ParseException("Failed to parse gallery detail", it)
      }

  fun parseComments(body: ByteBuffer) =
    Either.catch {
        if (Settings.useRustCore.value) {
          unmarshalParsingAs<ApiEnvelope<RustGalleryCommentsPage>>(body, ::nativeParseCommentsV1)
            .toGalleryCommentList()
        } else {
          unmarshalParsingAs<GalleryCommentList>(body, ::nativeParseComments)
        }
      }
      .getOrElse {
        if (it is RustFailureException) throw it
        throw ParseException("Failed to parse comments", it)
      }

  fun parsePreviews(body: ByteBuffer) =
    Either.catch {
        if (Settings.useRustCore.value) {
          unmarshalParsingAs<ApiEnvelope<RustGalleryPreviewPage>>(body, ::nativeParsePreviewsV1)
            .toGalleryPreviewList()
        } else {
          unmarshalParsingAs<GalleryPreviewList>(body, ::nativeParsePreviews)
        }
      }
      .getOrElse {
        if (it is RustFailureException) throw it
        throw ParseException("Failed to parse previews", it)
      }

  @Serializable @CborArray class Result(val detail: GalleryDetail, val event: String?)

  private external fun nativeParse(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes the V1 `eh_net::gallery::detail` parser. */
  private external fun nativeParseV1(body: ByteBuffer, size: Int = body.limit()): Int

  private external fun nativeParseComments(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes the V1 `eh_net::gallery::comments` parser. */
  private external fun nativeParseCommentsV1(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes the frozen, unversioned Gallery Preview rollback parser. */
  private external fun nativeParsePreviews(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes the V1 `eh_net::gallery::preview` parser. */
  private external fun nativeParsePreviewsV1(body: ByteBuffer, size: Int = body.limit()): Int
}
