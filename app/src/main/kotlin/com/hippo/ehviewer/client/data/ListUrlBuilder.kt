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
import com.hippo.ehviewer.bridge.gallery.CatalogNavigation
import com.hippo.ehviewer.bridge.gallery.CatalogNumbers
import com.hippo.ehviewer.bridge.gallery.CatalogTransition
import com.hippo.ehviewer.bridge.gallery.GALLERY_LIST_URL_API_VERSION
import com.hippo.ehviewer.bridge.gallery.GalleryListMode
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlRequest
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlState
import com.hippo.ehviewer.bridge.gallery.GalleryUrlSite
import com.hippo.ehviewer.bridge.gallery.buildGalleryListUrl
import com.hippo.ehviewer.bridge.gallery.catalogText
import com.hippo.ehviewer.bridge.gallery.savedSearchProgress
import com.hippo.ehviewer.bridge.gallery.transitionCatalog
import com.hippo.ehviewer.bridge.resolveQuickSearchMatch
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.EhUtils
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

  private fun navigate(
    action: CatalogTransition,
    text: String? = null,
    value: Int = 0,
    forward: Boolean = false,
  ) {
    val plan =
      transitionCatalog(
        CatalogNavigation.from(prev, next, jumpTo, range),
        action,
        text,
        value,
        forward,
      )
    prev = plan.previous?.catalogText()
    next = plan.next?.catalogText()
    jumpTo = plan.seek?.catalogText()
    range = plan.range
  }

  fun setIndex(index: String, isNext: Boolean = true) =
    navigate(CatalogTransition.INDEX, text = index, forward = isNext)

  var page: Int
    get() = CatalogNumbers.displayPage(jumpTo?.toInt() ?: 0)
    set(value) {
      navigate(CatalogTransition.PAGE, value = value)
    }

  fun setRange(to: Int) = navigate(CatalogTransition.RANGE, value = to)

  fun setSeek(date: String) = navigate(CatalogTransition.SEEK, text = date)

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
    next = savedSearchProgress(q.name),
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
        current = quickSearchSnapshot(),
        saved = q?.let(QuickSearchSnapshot::from),
      )
    return resolveQuickSearchMatch(request)
  }

  internal fun quickSearchSnapshot() =
    QuickSearchSnapshot(mode, category, keyword, advanceSearch, minRating, pageFrom, pageTo)

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
