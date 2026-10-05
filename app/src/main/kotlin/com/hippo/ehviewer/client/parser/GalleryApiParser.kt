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

import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryMetadataBatch
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.inferGalleryLanguage
import com.hippo.ehviewer.bridge.gallery.toGalleryMetadataBatch
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException

/** Parses and applies Gallery List metadata through the V1 Rust contract. */
object GalleryApiParser {
  /** Parses the complete response through `eh_net::gallery::metadata` without mutation. */
  fun parseV1(body: ByteBuffer): GalleryMetadataBatch = runCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryMetadataBatch>>(body, ::nativeParseGalleryMetadataV1)
      .toGalleryMetadataBatch()
  }
    .getOrElse {
      if (it is CancellationException) throw it
      if (it is RustFailureException) throw it
      throw ParseException("Failed to parse V1 gallery metadata", it)
    }

  /**
   * Applies a fully parsed V1 batch with the original first-match and source-order semantics.
   *
   * Duplicate response IDs update the same first target repeatedly, while duplicate target IDs
   * leave every target after the first unchanged.
   */
  internal fun applyV1(
    batch: GalleryMetadataBatch,
    galleryInfoList: List<GalleryInfo>,
    languageResolver: (String?, List<String>?) -> String? = ::inferGalleryLanguage,
  ) {
    batch.items.forEach { item ->
      val gi = galleryInfoList.find { it.gid == item.galleryId } ?: return@forEach
      gi.apply {
        title = item.title
        titleJpn = item.titleJapanese
        category = item.category
        thumbKey = item.thumbnailKey
        uploader = item.uploader
        posted = ParserUtils.formatDate(item.posted * 1000)
        rating = item.rating
        simpleTags = item.tags
        pages = item.pageCount
        generateSLang(languageResolver)
      }
    }
  }

  /** Invokes V1 parsing while requests, matching, formatting, and mutation stay on Android. */
  private external fun nativeParseGalleryMetadataV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
