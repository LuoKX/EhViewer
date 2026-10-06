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
package com.hippo.ehviewer.client

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.gallery.GalleryRequest
import com.hippo.ehviewer.bridge.gallery.GalleryRequestKind
import com.hippo.ehviewer.bridge.gallery.GalleryUrlSite
import com.hippo.ehviewer.bridge.gallery.planGalleryRequest
import io.ktor.http.Parameters
import io.ktor.http.ParametersBuilder
import io.ktor.http.URLBuilder
import io.ktor.http.URLProtocol

object EhUrl {
  const val SITE_E = 0
  const val SITE_EX = 1
  const val DOMAIN_EX = "exhentai.org"
  const val DOMAIN_E = "e-hentai.org"
  const val HOST_EX = "https://$DOMAIN_EX/"
  const val URL_UCONFIG_EX = HOST_EX + "uconfig.php"
  const val URL_MY_TAGS_EX = HOST_EX + "mytags"
  const val HOST_E = "https://$DOMAIN_E/"
  const val URL_UCONFIG_E = HOST_E + "uconfig.php"
  const val URL_MY_TAGS_E = HOST_E + "mytags"
  const val API_SIGN_IN = "https://forums.e-hentai.org/index.php?act=Login&CODE=01"
  const val URL_SIGN_IN = "https://forums.e-hentai.org/index.php?act=Login"
  const val URL_REGISTER = "https://forums.e-hentai.org/index.php?act=Reg&CODE=00"
  const val URL_FORUMS = "https://forums.e-hentai.org/"
  const val URL_FUNDS = HOST_E + "exchange.php?t=gp"
  const val URL_HOME = HOST_E + "home.php"
  const val URL_NEWS = HOST_E + "news.php"
  const val REFERER_EX = "https://$DOMAIN_EX"
  const val ORIGIN_EX = REFERER_EX
  const val REFERER_E = "https://$DOMAIN_E"
  const val ORIGIN_E = REFERER_E

  val domain: String
    get() =
      when (Settings.gallerySite.value) {
        SITE_E -> DOMAIN_E
        SITE_EX -> DOMAIN_EX
        else -> DOMAIN_E
      }

  val referer: String
    get() =
      when (Settings.gallerySite.value) {
        SITE_E -> REFERER_E
        SITE_EX -> REFERER_EX
        else -> REFERER_E
      }

  val origin: String
    get() =
      when (Settings.gallerySite.value) {
        SITE_E -> ORIGIN_E
        SITE_EX -> ORIGIN_EX
        else -> ORIGIN_E
      }

  fun getUConfigUrl(gallerySite: Int = Settings.gallerySite.value) =
    when (gallerySite) {
      SITE_E -> URL_UCONFIG_E
      SITE_EX -> URL_UCONFIG_EX
      else -> URL_UCONFIG_E
    }

  val myTagsUrl: String
    get() =
      when (Settings.gallerySite.value) {
        SITE_E -> URL_MY_TAGS_E
        SITE_EX -> URL_MY_TAGS_EX
        else -> URL_MY_TAGS_E
      }

  internal val requestSite: GalleryUrlSite
    get() =
      if (Settings.gallerySite.value == SITE_EX) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC

  fun getGalleryDetailUrl(gid: Long, token: String, index: Int = 0, allComment: Boolean = false) =
    planGalleryRequest(
        GalleryRequest(
          1,
          requestSite,
          GalleryRequestKind.DETAIL_URL,
          gid = gid,
          token = token,
          index = index,
          allComments = allComment,
        )
      )
      .url

  fun getGalleryMultiPageViewerUrl(gid: Long, token: String) =
    planGalleryRequest(
        GalleryRequest(1, requestSite, GalleryRequestKind.MPV_URL, gid = gid, token = token)
      )
      .url

  fun getPageUrl(gid: Long, index: Int, pToken: String, nl: String? = null) =
    planGalleryRequest(
        GalleryRequest(
          1,
          requestSite,
          GalleryRequestKind.PAGE_URL,
          gid = gid,
          index = index,
          token = pToken,
          nl = nl,
        )
      )
      .url

  fun getAddFavorites(gid: Long, token: String) =
    ehUrl("gallerypopups.php") {
        addQueryParameter("gid", "$gid")
        addQueryParameter("t", token)
        addQueryParameter("act", "addfav")
      }
      .buildString()

  fun getTorrentUrl(gid: Long, token: String) =
    ehUrl("gallerytorrents.php") {
        addQueryParameter("gid", "$gid")
        addQueryParameter("t", token)
      }
      .buildString()

  fun getTagDefinitionUrl(tag: String) = "https://ehwiki.org/wiki/" + tag.replace(' ', '_')

  fun getTrackerUrl(gid: Long, key: String?) =
    ehUrl(
        path = listOfNotNull("$gid", key, "announce"),
        host = "ehtracker.org",
      )
      .buildString()
}

inline fun ehUrl(
  path: String? = null,
  host: String = EhUrl.domain,
  builder: ParametersBuilder.() -> Unit,
) =
  ehUrl(
    path = if (path.isNullOrBlank()) emptyList() else listOf(path),
    host = host,
    parameters = Parameters.build(builder),
  )

inline fun ehUrl(
  path: List<String>,
  host: String = EhUrl.domain,
  builder: ParametersBuilder.() -> Unit,
) =
  ehUrl(
    path = path,
    host = host,
    parameters = Parameters.build(builder),
  )

fun ehUrl(
  path: List<String>,
  host: String = EhUrl.domain,
  parameters: Parameters = Parameters.Empty,
) =
  URLBuilder(
    protocol = URLProtocol.HTTPS,
    host = host,
    pathSegments = path,
    parameters = parameters,
  )

fun ParametersBuilder.addQueryParameter(name: String, value: String) = append(name, value)
