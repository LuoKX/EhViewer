/*
 * Copyright (C) 2015 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.client.data

import com.ehviewer.core.database.model.QuickSearch
import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.LIBRARY_RULE_API_VERSION
import com.hippo.ehviewer.bridge.QuickSearchMatchRequest
import com.hippo.ehviewer.bridge.QuickSearchSnapshot
import com.hippo.ehviewer.bridge.gallery.GALLERY_LIST_URL_API_VERSION
import com.hippo.ehviewer.bridge.gallery.GalleryListMode
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlRequest
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlState
import com.hippo.ehviewer.bridge.gallery.GalleryUrlSite
import com.hippo.ehviewer.bridge.gallery.buildGalleryListUrl
import com.hippo.ehviewer.bridge.resolveQuickSearchMatch
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.EhUtils
import com.hippo.ehviewer.ui.main.AdvanceTable
import io.ktor.http.Parameters
import kotlinx.serialization.Serializable

@Serializable
data class ListUrlBuilder(
  var mode: Int = MODE_NORMAL,
  private var prev: String? = null,
  private var next: String? = null,
  private var jumpTo: String? = null,
  private var range: Int = 0,
  var category: Int = EhUtils.NONE,
  var keyword: String? = null,
  var hash: String? = null,
  var language: Int = -1,
  var advanceSearch: Int = -1,
  var minRating: Int = -1,
  var pageFrom: Int = -1,
  var pageTo: Int = -1,
) {

  fun setIndex(index: String, isNext: Boolean = true) {
    next = index.takeIf { isNext }
    prev = index.takeUnless { isNext }
    range = 0
    jumpTo = null
  }

  var page: Int
    get() = (jumpTo?.toInt() ?: 0) + 1
    set(value) {
      jumpTo = (value - 1).takeUnless { it == 0 }?.toString()
    }

  fun setRange(to: Int) {
    range = to
    prev = null
    next = null
    jumpTo = null
  }

  fun setSeek(date: String) {
    setIndex("2", true)
    jumpTo = date
  }

  constructor(
    q: QuickSearch
  ) : this(
    mode = q.mode,
    category = q.category,
    keyword = q.keyword,
    advanceSearch = q.advanceSearch,
    minRating = q.minRating,
    pageFrom = q.pageFrom,
    pageTo = q.pageTo,
    next = q.name.substringAfterLast('@', "").ifEmpty { null },
  )

  fun toQuickSearch(name: String) =
    QuickSearch(
      name = name,
      mode = mode,
      category = category,
      keyword = keyword,
      advanceSearch = advanceSearch,
      minRating = minRating,
      pageFrom = pageFrom,
      pageTo = pageTo,
    )

  fun equalsQuickSearch(q: QuickSearch?): Boolean {
    val request =
      QuickSearchMatchRequest(
        version = LIBRARY_RULE_API_VERSION,
        current =
          QuickSearchSnapshot(
            mode,
            category,
            keyword,
            advanceSearch,
            minRating,
            pageFrom,
            pageTo,
          ),
        saved = q?.let(QuickSearchSnapshot::from),
      )
    return resolveQuickSearchMatch(request)
  }

  constructor(
    params: Parameters
  ) : this(
    prev = params["prev"],
    next = params["next"],
    jumpTo = params["seek"],
    range = params["range"]?.toIntOrNull() ?: 0,
    category = params["f_cats"]?.toIntOrNull()?.let(EhUtils::invCategory) ?: EhUtils.NONE,
    keyword = params["f_search"],
  ) {
    if (params["advsearch"] == "1") {
      advanceSearch = 0
      if (params["f_sh"] == "on") {
        advanceSearch = advanceSearch or AdvanceTable.SH
      }
      if (params["f_sto"] == "on") {
        advanceSearch = advanceSearch or AdvanceTable.STO
      }
      if (params["f_sfl"] == "on") {
        advanceSearch = advanceSearch or AdvanceTable.SFL
      }
      if (params["f_sfu"] == "on") {
        advanceSearch = advanceSearch or AdvanceTable.SFU
      }
      if (params["f_sft"] == "on") {
        advanceSearch = advanceSearch or AdvanceTable.SFT
      }
      minRating = params["f_srdd"]?.toIntOrNull() ?: -1
      pageFrom = params["f_spf"]?.toIntOrNull() ?: -1
      pageTo = params["f_spt"]?.toIntOrNull() ?: -1
    }
  }

  /** Returns the complete platform-neutral state compared by the URL parser rollout adapter. */
  internal fun toGalleryListUrlState() =
    GalleryListUrlState(
      mode = mode,
      previous = prev,
      next = next,
      seek = jumpTo,
      range = range,
      category = category,
      keyword = keyword,
      imageHash = hash,
      language = language,
      advancedSearch = advanceSearch,
      minimumRating = minRating,
      pageFrom = pageFrom,
      pageTo = pageTo,
    )

  fun build(): String = buildGalleryListUrl(rustUrlRequest())

  private fun rustUrlRequest() =
    GalleryListUrlRequest(
      version = GALLERY_LIST_URL_API_VERSION,
      site =
        if (Settings.gallerySite.value == EhUrl.SITE_EX) GalleryUrlSite.EX_HENTAI
        else GalleryUrlSite.PUBLIC,
      signedIn = Settings.hasSignedIn.value,
      mode =
        when (mode) {
          MODE_NORMAL -> GalleryListMode.NORMAL
          MODE_UPLOADER -> GalleryListMode.UPLOADER
          MODE_TAG -> GalleryListMode.TAG
          MODE_WHATS_HOT -> GalleryListMode.WHATS_HOT
          MODE_IMAGE_SEARCH -> GalleryListMode.IMAGE_SEARCH
          MODE_SUBSCRIPTION -> GalleryListMode.SUBSCRIPTION
          MODE_TOPLIST -> GalleryListMode.TOPLIST
          else -> error("Unexpected value: $mode")
        },
      previous = prev,
      next = next,
      seek = jumpTo,
      range = range,
      category = category,
      keyword = keyword,
      imageHash = hash,
      languageTag = language.takeIf { it >= 0 }?.let(GalleryInfo.S_LANG_TAGS::get),
      advancedSearch = advanceSearch,
      minimumRating = minRating,
      pageFrom = pageFrom,
      pageTo = pageTo,
    )

  companion object {
    const val MODE_NORMAL = 0x0
    const val MODE_UPLOADER = 0x1
    const val MODE_TAG = 0x2
    const val MODE_WHATS_HOT = 0x3
    const val MODE_IMAGE_SEARCH = 0x4
    const val MODE_SUBSCRIPTION = 0x5
    const val MODE_TOPLIST = 0x6

    /** Restores the complete legacy builder state emitted by the Rust URL parser. */
    internal fun fromGalleryListUrlState(state: GalleryListUrlState) =
      ListUrlBuilder(
        mode = state.mode,
        prev = state.previous,
        next = state.next,
        jumpTo = state.seek,
        range = state.range,
        category = state.category,
        keyword = state.keyword,
        hash = state.imageHash,
        language = state.language,
        advanceSearch = state.advancedSearch,
        minRating = state.minimumRating,
        pageFrom = state.pageFrom,
        pageTo = state.pageTo,
      )
  }
}
