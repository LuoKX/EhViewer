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
import com.ehviewer.core.util.unescapeXml
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryMetadataBatch
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toGalleryMetadataBatch
import com.hippo.ehviewer.client.EhUtils.getCategory
import com.hippo.ehviewer.client.exception.ParseException
import com.hippo.ehviewer.client.getThumbKey
import com.hippo.ehviewer.client.parseAs
import java.nio.ByteBuffer
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Parses and applies Gallery List metadata through the rollback or V1 Rust contract. */
object GalleryApiParser {
  /** Preserves the original Kotlin parse-and-mutate path for immediate rollback. */
  fun parse(body: String, galleryInfoList: List<GalleryInfo>) {
    body.parseAs<Result>().items.forEach { item ->
      val gi = galleryInfoList.find { it.gid == item.gid } ?: return@forEach
      gi.apply {
        title = item.title.unescapeXml()
        titleJpn = item.titleJpn.unescapeXml()
        category = getCategory(item.category)
        thumbKey = getThumbKey(item.thumb)
        uploader = item.uploader?.unescapeXml()
        posted = ParserUtils.formatDate(item.posted * 1000)
        rating = item.rating
        simpleTags = item.tags
        pages = item.pages
        generateSLang()
      }
    }
  }

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
  internal fun applyV1(batch: GalleryMetadataBatch, galleryInfoList: List<GalleryInfo>) {
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
        generateSLang()
      }
    }
  }

  /** Selects exactly one parser before the HTTP response body is consumed. */
  internal suspend fun parseWithAdapters(
    useRust: Boolean,
    legacyParser: suspend () -> Unit,
    rustParser: suspend () -> Unit,
  ) {
    if (useRust) rustParser() else legacyParser()
  }

  /** Invokes V1 parsing while requests, matching, formatting, and mutation stay on Android. */
  private external fun nativeParseGalleryMetadataV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int

  /** Legacy serializer root retained as the default-off rollback contract. */
  @Serializable data class Result(@SerialName("gmetadata") val items: List<Item>)

  /** Legacy serializer item retained byte-for-byte in behavior for rollback. */
  @Serializable
  data class Item(
    val gid: Long,
    val title: String,
    @SerialName("title_jpn") val titleJpn: String,
    val category: String,
    val thumb: String,
    // Null in some old galleries.
    val uploader: String?,
    val posted: Long,
    @SerialName("filecount") val pages: Int,
    val rating: Float,
    val tags: List<String>,
  )
}
