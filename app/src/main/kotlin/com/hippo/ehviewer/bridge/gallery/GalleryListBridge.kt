package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.GalleryListResult

/** Validates and adapts the V1 `eh_net::gallery::list` response to the Android model. */
internal fun ApiEnvelope<GalleryPage>.toGalleryListResult(): GalleryListResult {
  require(version == GALLERY_LIST_API_VERSION) {
    "Unsupported Gallery List API version: $version"
  }
  require(module == GALLERY_LIST_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Gallery List response has no payload" }
  return page.toGalleryListResult()
}

/** Maps the shared Gallery List page payload without imposing an envelope discriminator. */
internal fun GalleryPage.toGalleryListResult(): GalleryListResult =
  GalleryListResult(
    prev = previous,
    next = next,
    galleryInfoList =
      items.mapTo(ArrayList(items.size)) { item ->
        item.toBaseGalleryInfo()
      },
  )

private const val GALLERY_LIST_API_VERSION = 1
private const val GALLERY_LIST_MODULE = "galleryList"
