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
package com.ehviewer.core.model

interface GalleryInfo {
  var gid: Long
  var token: String
  var title: String?
  var titleJpn: String?
  var thumbKey: String?
  var category: Int
  var posted: String?
  var uploader: String?
  var disowned: Boolean
  var rating: Float
  var rated: Boolean
  var simpleTags: List<String>?
  var pages: Int
  var thumbWidth: Int
  var thumbHeight: Int
  var simpleLanguage: String?
  var favoriteSlot: Int
  var favoriteName: String?
  var favoriteNote: String?

  /** Assigns a language chosen by the application-owned Rust boundary. */
  fun generateSLang(resolve: (String?, List<String>?) -> String?) {
    simpleLanguage = resolve(title, simpleTags)
  }

  companion object {
    val S_LANGS =
      arrayOf(
        "EN",
        "ZH",
        "ES",
        "KO",
        "RU",
        "FR",
        "PT",
        "TH",
        "DE",
        "IT",
        "VI",
        "PL",
        "HU",
        "NL",
      )
    val S_LANG_TAGS =
      arrayOf(
        "language:english",
        "language:chinese",
        "language:spanish",
        "language:korean",
        "language:russian",
        "language:french",
        "language:portuguese",
        "language:thai",
        "language:german",
        "language:italian",
        "language:vietnamese",
        "language:polish",
        "language:hungarian",
        "language:dutch",
      )

    const val NOT_FAVORITED = -2
    const val LOCAL_FAVORITED = -1
  }
}
