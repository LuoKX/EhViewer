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
import com.hippo.ehviewer.bridge.gallery.GALLERY_LIST_URL_PARSER_API_VERSION
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlParameters
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlParseRequest
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlSnapshot
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlState
import com.hippo.ehviewer.bridge.gallery.parseGalleryListUrl
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.data.ListUrlBuilder
import io.ktor.http.Url

object GalleryListUrlParser {
  fun parse(urlStr: String) =
    parseWithRustAdapter(
      urlStr,
      rustParser = ::parseGalleryListUrl,
    )

  /** Runs exactly one selected parser after Android's URL syntax and host preflight. */
  internal fun parseWithRustAdapter(
    urlStr: String,
    rustParser: (GalleryListUrlParseRequest) -> GalleryListUrlState?,
  ): ListUrlBuilder? {
    val url = Either.catch { Url(urlStr) }.getOrNull() ?: return null
    if (url.host != EhUrl.DOMAIN_E && url.host != EhUrl.DOMAIN_EX) return null
    return rustParser(url.toRustRequest())?.let { ListUrlBuilder.fromGalleryListUrlState(it) }
  }

  private fun Url.toRustRequest() =
    GalleryListUrlParseRequest(
      version = GALLERY_LIST_URL_PARSER_API_VERSION,
      snapshot =
        GalleryListUrlSnapshot(
          segments = segments.take(2),
          parameters =
            GalleryListUrlParameters(
              imageHash = parameters["f_shash"],
              previous = parameters["prev"],
              next = parameters["next"],
              seek = parameters["seek"],
              range = parameters["range"],
              excludedCategories = parameters["f_cats"],
              search = parameters["f_search"],
              advancedSearch = parameters["advsearch"],
              searchName = parameters["f_sh"],
              searchTags = parameters["f_sto"],
              searchDescription = parameters["f_sfl"],
              searchTorrentFilenames = parameters["f_sfu"],
              searchLowPowerTags = parameters["f_sft"],
              minimumRating = parameters["f_srdd"],
              pageFrom = parameters["f_spf"],
              pageTo = parameters["f_spt"],
              toplist = parameters["tl"],
              toplistPage = parameters["p"],
            ),
        ),
    )
}
