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

import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.bridge.gallery.CacheKeyKind
import com.hippo.ehviewer.bridge.gallery.CacheKeyRequest
import com.hippo.ehviewer.bridge.gallery.planCacheKey

// Normal Preview (v2): https://*.hath.network/c(m|1|2)/[timed token]/[gid]-[index].(jpg|webp)
// ExHentai Large Preview (v1 Cover): https://s.exhentai.org/t/***
// E-Hentai Large Preview (v1 Cover): https://ehgt.org/***
// ExHentai v2 Cover: https://s.exhentai.org/**.webp
// E-Hentai v2 Cover: https://ehgt.org/**.webp

const val URL_PREFIX_THUMB_E = "https://ehgt.org/"
const val URL_PREFIX_THUMB_EX = "https://s.exhentai.org/"

fun getImageKey(gid: Long, index: Int): String =
  planCacheKey(CacheKeyRequest(kind = CacheKeyKind.IMAGE, gid = gid, index = index))

fun getThumbKey(url: String): String =
  planCacheKey(CacheKeyRequest(kind = CacheKeyKind.THUMB, input = url))

fun getV2PreviewKey(url: String): String =
  planCacheKey(CacheKeyRequest(kind = CacheKeyKind.PREVIEW, input = url))

val GalleryInfo.thumbUrl
  get() = keyToUrl(thumbKey!!)

fun keyToUrl(key: String): String =
  planCacheKey(CacheKeyRequest(kind = CacheKeyKind.URL, input = key, exHentai = EhUtils.isExHentai))
