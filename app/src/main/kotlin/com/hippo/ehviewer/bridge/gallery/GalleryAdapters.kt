package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.bridge.GalleryItemSummary

/** Maps the shared Rust gallery-summary contract to the existing Android cache/UI model. */
internal fun GalleryItemSummary.toBaseGalleryInfo(): BaseGalleryInfo =
  BaseGalleryInfo(
    gid = galleryId,
    token = token,
    title = title,
    titleJpn = titleJapanese,
    thumbKey = thumbnailKey,
    category = category,
    posted = posted,
    uploader = uploader,
    disowned = disowned,
    rating = rating,
    rated = rated,
    simpleTags = tags,
    pages = pageCount,
    thumbWidth = thumbnailWidth,
    thumbHeight = thumbnailHeight,
    simpleLanguage = language,
    favoriteSlot = favoriteSlot,
    favoriteName = favoriteName,
    favoriteNote = favoriteNote,
  )
