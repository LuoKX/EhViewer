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
package com.hippo.ehviewer.client.data

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.gallery.CatalogNavigation
import com.hippo.ehviewer.bridge.gallery.CatalogTransition
import com.hippo.ehviewer.bridge.gallery.FAVORITES_URL_API_VERSION
import com.hippo.ehviewer.bridge.gallery.FavoritesUrlRequest
import com.hippo.ehviewer.bridge.gallery.GalleryUrlSite
import com.hippo.ehviewer.bridge.gallery.buildFavoritesUrl
import com.hippo.ehviewer.bridge.gallery.catalogText
import com.hippo.ehviewer.bridge.gallery.transitionCatalog
import com.hippo.ehviewer.client.EhUrl
import kotlinx.serialization.Serializable

@Serializable
data class FavListUrlBuilder(
  val favCat: Int = FAV_CAT_ALL,
  val keyword: String? = null,
  private var jumpTo: String? = null,
  private var prev: String? = null,
  private var next: String? = null,
) {
  val isLocal
    get() = favCat == FAV_CAT_LOCAL

  fun setIndex(index: String?, isNext: Boolean) {
    val plan =
      transitionCatalog(
        CatalogNavigation.from(prev, next, jumpTo),
        CatalogTransition.INDEX,
        index,
        forward = isNext,
      )
    prev = plan.previous?.catalogText()
    next = plan.next?.catalogText()
    jumpTo = plan.seek?.catalogText()
  }

  fun build(): String = buildFavoritesUrl(rustUrlRequest())

  /** Return a new builder so the existing refresh/SavedState flow observes the navigation. */
  fun seek(date: String): FavListUrlBuilder = copyNavigation(CatalogTransition.SEEK, date)

  fun lastPage(): FavListUrlBuilder = copyNavigation(CatalogTransition.INDEX, "1-0")

  private fun copyNavigation(action: CatalogTransition, text: String): FavListUrlBuilder {
    val plan = transitionCatalog(CatalogNavigation.from(prev, next, jumpTo), action, text)
    return copy(
      prev = plan.previous?.catalogText(),
      next = plan.next?.catalogText(),
      jumpTo = plan.seek?.catalogText(),
    )
  }

  private fun rustUrlRequest() =
    FavoritesUrlRequest(
      version = FAVORITES_URL_API_VERSION,
      site =
        if (Settings.gallerySite.value == EhUrl.SITE_EX) GalleryUrlSite.EX_HENTAI
        else GalleryUrlSite.PUBLIC,
      favoriteCategory = favCat,
      keyword = keyword,
      previous = prev,
      next = next,
      seek = jumpTo,
    )

  companion object {
    const val FAV_CAT_ALL = -1
    const val FAV_CAT_LOCAL = -2
  }
}
