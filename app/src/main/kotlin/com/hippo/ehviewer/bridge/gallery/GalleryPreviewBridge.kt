package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.GalleryPreviewList
import com.ehviewer.core.model.V1GalleryPreview
import com.ehviewer.core.model.V2GalleryPreview
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryPreviewKind
import com.hippo.ehviewer.bridge.GalleryPreviewPage
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates and adapts the V1 gallery-preview response to the existing Android model. */
internal fun ApiEnvelope<GalleryPreviewPage>.toGalleryPreviewList(): GalleryPreviewList {
  require(version == GALLERY_PREVIEW_API_VERSION) {
    "Unsupported Gallery Preview API version: $version"
  }
  require(module == GALLERY_PREVIEW_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Gallery Preview response has no payload" }
  return page.toGalleryPreviewList()
}

/** Validates and maps a nested Gallery Preview payload. */
internal fun GalleryPreviewPage.toGalleryPreviewList(): GalleryPreviewList {
  require(total > 0) { "Gallery Preview total must be positive" }
  require(previews.isNotEmpty()) { "Gallery Preview payload must not be empty" }
  return GalleryPreviewList(
    previews =
      previews.map { preview ->
        require(preview.url.isNotBlank()) { "Gallery Preview URL must not be blank" }
        require(preview.position in 0 until total) {
          "Gallery Preview position must be within the gallery page count"
        }
        when (preview.kind) {
          GalleryPreviewKind.V1 -> {
            require(
              preview.offsetX == null && preview.clipWidth == null && preview.clipHeight == null
            ) {
              "V1 Gallery Preview must not contain sprite geometry"
            }
            V1GalleryPreview(preview.url, preview.position, preview.pageToken)
          }

          GalleryPreviewKind.V2 -> {
            val offsetX = checkNotNull(preview.offsetX) { "V2 Gallery Preview needs offsetX" }
            val clipWidth = checkNotNull(preview.clipWidth) { "V2 Gallery Preview needs clipWidth" }
            val clipHeight =
              checkNotNull(preview.clipHeight) { "V2 Gallery Preview needs clipHeight" }
            require(offsetX >= 0 && clipWidth > 0 && clipHeight > 0) {
              "V2 Gallery Preview geometry is invalid"
            }
            V2GalleryPreview(
              preview.url,
              preview.position,
              preview.pageToken,
              offsetX,
              clipWidth,
              clipHeight,
            )
          }
        }
      },
    total = total,
  )
}

private const val GALLERY_PREVIEW_API_VERSION = 1
private const val GALLERY_PREVIEW_MODULE = "galleryPreview"
