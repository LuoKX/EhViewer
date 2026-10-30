package com.ehviewer.core.database.dao

import com.ehviewer.core.database.model.GalleryEntity

/** Pure synchronous decisions invoked at their original points inside the promotion transaction. */
interface RecoveryMergePolicy {
  fun mergeGallery(existing: GalleryEntity?, incoming: GalleryEntity): GalleryEntity

  fun mergeArtists(existing: List<String>, incoming: List<String>): List<String>
}
