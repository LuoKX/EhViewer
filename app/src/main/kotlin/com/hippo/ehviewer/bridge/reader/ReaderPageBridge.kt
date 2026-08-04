package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ReaderPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.GalleryPageParser

/** Validates and adapts the V1 reader-page contract to the existing parser result. */
internal fun ApiEnvelope<ReaderPage>.toGalleryPageResult(): GalleryPageParser.Result {
  require(version == READER_PAGE_API_VERSION) { "Unsupported Reader Page API version: $version" }
  require(module == READER_PAGE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Reader Page response has no payload" }
  require(page.imageUrl.isNotBlank()) { "Reader Page image URL must not be blank" }
  require(page.skipHathKey == null || page.skipHathKey.isNotBlank()) {
    "Reader Page H@H bypass key must not be blank"
  }
  require(page.originalImageUrl == null || page.originalImageUrl.isNotBlank()) {
    "Reader Page original image URL must not be blank"
  }
  require(page.showKey == null || page.showKey.isNotBlank()) {
    "Reader Page show key must not be blank"
  }
  return GalleryPageParser.Result(
    imageUrl = page.imageUrl,
    skipHathKey = page.skipHathKey,
    originImageUrl = page.originalImageUrl,
    showKey = page.showKey,
  )
}

private const val READER_PAGE_API_VERSION = 1
private const val READER_PAGE_MODULE = "readerPage"
