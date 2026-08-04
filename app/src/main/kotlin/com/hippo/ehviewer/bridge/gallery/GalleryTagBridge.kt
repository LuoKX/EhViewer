package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.GalleryTag as AndroidGalleryTag
import com.ehviewer.core.model.GalleryTagGroup as AndroidGalleryTagGroup
import com.ehviewer.core.model.PowerStatus
import com.ehviewer.core.model.TagNamespace
import com.ehviewer.core.model.VoteStatus
import com.hippo.ehviewer.bridge.GalleryTagGroup
import com.hippo.ehviewer.bridge.GalleryTagPower
import com.hippo.ehviewer.bridge.GalleryTagVote

/** Maps versioned Rust tag groups while preserving each response grammar's text rule. */
internal fun List<GalleryTagGroup>.toAndroidGalleryTagGroups(
  operation: String,
  requireNonBlankText: Boolean,
): List<AndroidGalleryTagGroup> = map { group ->
  val namespace =
    checkNotNull(TagNamespace.from(group.namespace)) {
      "Unknown $operation tag namespace: ${group.namespace}"
    }
  AndroidGalleryTagGroup(
    namespace = namespace,
    tags =
      group.tags.map { tag ->
        if (requireNonBlankText) {
          require(tag.text.isNotBlank()) { "$operation tag text must not be blank" }
        }
        AndroidGalleryTag(
          text = tag.text,
          power =
            when (tag.power) {
              GalleryTagPower.SOLID -> PowerStatus.Solid
              GalleryTagPower.ACTIVE -> PowerStatus.Active
              GalleryTagPower.WEAK -> PowerStatus.Weak
            },
          vote =
            when (tag.vote) {
              GalleryTagVote.NONE -> VoteStatus.None
              GalleryTagVote.UP -> VoteStatus.Up
              GalleryTagVote.DOWN -> VoteStatus.Down
            },
        )
      },
  )
}
