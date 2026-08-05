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

import com.hippo.ehviewer.bridge.gallery.GALLERY_NAVIGATION_API_VERSION
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationKind
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationPlan
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationRequest
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationTarget
import com.hippo.ehviewer.bridge.gallery.parseGalleryNavigation

/** Like http://exhentai.org/s/91ea4b6d89/901103-12 */
object GalleryPageUrlParser {
  fun parse(url: String?, strict: Boolean = true) =
    parseWithRustAdapter(
      url = url,
      strict = strict,
      rustParser = ::parseGalleryNavigation,
    )

  /** Runs exactly one selected parser. */
  internal fun parseWithRustAdapter(
    url: String?,
    strict: Boolean,
    rustParser: (GalleryNavigationRequest) -> GalleryNavigationPlan,
  ): Result? {
    url ?: return null
    val plan =
      rustParser(
        GalleryNavigationRequest(
          version = GALLERY_NAVIGATION_API_VERSION,
          target = GalleryNavigationTarget.PAGE,
          input = url,
          strict = strict,
        )
      )
    require(plan.kind == GalleryNavigationKind.NONE || plan.kind == GalleryNavigationKind.PAGE) {
      "Rust Gallery Navigation page parser returned the wrong target"
    }
    if (plan.kind == GalleryNavigationKind.NONE) return null
    return Result(checkNotNull(plan.gid), checkNotNull(plan.token), checkNotNull(plan.pageIndex))
  }

  data class Result(val gid: Long, val pToken: String, val page: Int)
}
