package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.BaseGalleryInfo
import com.ehviewer.core.model.GalleryDetail as AndroidGalleryDetail
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryDetailPage
import com.hippo.ehviewer.bridge.GalleryItemSummary
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.GalleryDetailParser

/** Validates and adapts the V1 rich Gallery Detail response to the Android parser result. */
internal fun ApiEnvelope<GalleryDetailPage>.toGalleryDetailResult(): GalleryDetailParser.Result {
  require(version == GALLERY_DETAIL_API_VERSION) {
    "Unsupported Gallery Detail API version: $version"
  }
  require(module == GALLERY_DETAIL_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Gallery Detail response has no payload" }
  val metadata = page.detail
  validateGalleryInfo(metadata.galleryInfo)
  require(metadata.apiUid >= -1) { "Gallery Detail API user identifier is invalid" }
  require(metadata.apiKey == null || metadata.apiKey.isNotBlank()) {
    "Gallery Detail API key must not be blank"
  }
  require(metadata.torrentCount >= 0) { "Gallery Detail torrent count must not be negative" }
  require(metadata.favoriteCount >= 0) { "Gallery Detail favorite count must not be negative" }
  require(metadata.ratingCount >= 0) { "Gallery Detail rating count must not be negative" }

  val comments = page.comments.toGalleryCommentList()
  val previews = page.previews.toGalleryPreviewList()
  require(metadata.galleryInfo.pageCount == previews.total) {
    "Gallery Detail and preview page counts do not match"
  }
  val tagGroups =
    page.tagGroups.toAndroidGalleryTagGroups(
      operation = "Gallery Detail",
      requireNonBlankText = true,
    )
  val detail =
    AndroidGalleryDetail(
      galleryInfo = metadata.galleryInfo.toBaseGalleryInfo(),
      apiUid = metadata.apiUid,
      apiKey = metadata.apiKey,
      torrentCount = metadata.torrentCount,
      parent = metadata.parent,
      newerVersions =
        metadata.newerVersions.map { version ->
          require(version.galleryId > 0 && version.token.isNotBlank()) {
            "Gallery Detail newer-version key is invalid"
          }
          BaseGalleryInfo(
            gid = version.galleryId,
            token = version.token,
            title = version.title,
            posted = version.posted,
          )
        },
      visible = metadata.visible,
      language = metadata.language,
      size = metadata.size,
      favoriteCount = metadata.favoriteCount,
      ratingCount = metadata.ratingCount,
      tagGroups = tagGroups,
      comments = comments,
      previewList = previews.previews,
    )
  return GalleryDetailParser.Result(detail, page.eventHtml)
}

private fun validateGalleryInfo(info: GalleryItemSummary) {
  require(info.galleryId > 0) { "Gallery Detail identifier must be positive" }
  require(info.token.isNotBlank()) { "Gallery Detail token must not be blank" }
  require(info.title.isNotBlank()) { "Gallery Detail title must not be blank" }
  require(info.thumbnailKey.isNotBlank()) { "Gallery Detail thumbnail key must not be blank" }
  require(info.pageCount > 0) { "Gallery Detail page count must be positive" }
  require(info.favoriteSlot >= -2) { "Gallery Detail favorite slot is invalid" }
}

private const val GALLERY_DETAIL_API_VERSION = 1
private const val GALLERY_DETAIL_MODULE = "galleryDetail"
