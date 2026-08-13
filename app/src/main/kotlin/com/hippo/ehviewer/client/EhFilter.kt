package com.hippo.ehviewer.client

import arrow.core.memoize
import com.ehviewer.core.database.model.Filter
import com.ehviewer.core.model.GalleryComment
import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.bridge.gallery.GalleryFilterComment
import com.hippo.ehviewer.bridge.gallery.GalleryFilterGallery
import com.hippo.ehviewer.bridge.gallery.GalleryFilterOperation
import com.hippo.ehviewer.bridge.gallery.GalleryFilterRequest
import com.hippo.ehviewer.bridge.gallery.GalleryFilterSnapshot
import com.hippo.ehviewer.bridge.gallery.planGalleryFilter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.launch

/**
 * Kotlin [Regex] delegates to `java.util.regex.Pattern`; Android libcore implements that public API
 * with ICU4C. Keeping compilation on Android preserves the stored-pattern behavior that Rust cannot
 * reproduce exactly.
 */
private val androidIcuRegex = { pattern: String -> Regex(pattern) }.memoize()

object EhFilter : CoroutineScope {
  override val coroutineContext = Dispatchers.IO.limitedParallelism(1)
  val filters = async { EhDB.getAllFilter() as MutableList }

  private fun <R> Filter.launchOps(
    callback: (suspend (R) -> Unit)? = null,
    ops: suspend Filter.() -> R,
  ) = launch { ops().let { callback?.invoke(it) } }

  fun Filter.remember(callback: (suspend (Boolean) -> Unit)? = null) =
    launchOps(callback) {
      EhDB.addFilter(this).also { if (it) filters.await().add(this) }
    }

  fun Filter.trigger(callback: (suspend (Unit) -> Unit)? = null) =
    launchOps(callback) {
      enable = !enable
      EhDB.updateFilter(this)
    }

  fun Filter.forget(callback: (suspend (Unit) -> Unit)? = null) =
    launchOps(callback) {
      EhDB.deleteFilter(this)
      filters.await().remove(this)
    }

  /** Applies the Rust pre-enrichment plan and returns its metadata requirement. */
  suspend fun <T : GalleryInfo> filterPreEnrichment(galleries: MutableList<T>): Boolean {
    val filterSnapshots = snapshots()
    val plan =
      planGalleryFilter(
        GalleryFilterRequest.galleries(
          GalleryFilterOperation.PRE_ENRICHMENT,
          filterSnapshots,
          galleries.map { it.toFilterGallery() },
        )
      )
    galleries.applyKeepPlan(plan.keep)
    return plan.needsTags
  }

  /** Applies the Rust post-enrichment plan exactly once. */
  suspend fun <T : GalleryInfo> filterPostEnrichment(galleries: MutableList<T>) {
    val filterSnapshots = snapshots()
    val plan =
      planGalleryFilter(
        GalleryFilterRequest.galleries(
          GalleryFilterOperation.POST_ENRICHMENT,
          filterSnapshots,
          galleries.map { it.toFilterGallery() },
        )
      )
    galleries.applyKeepPlan(plan.keep)
  }

  /** Returns the comment rows retained by one Rust plan. */
  suspend fun filterComments(
    comments: List<GalleryComment>,
    scoreThreshold: Int,
  ): List<GalleryComment> {
    val filterSnapshots = snapshots()
    val commentSnapshots = comments.map { GalleryFilterComment(it.score, it.uploader, it.user) }
    val preflight =
      planGalleryFilter(
        GalleryFilterRequest.comments(
          filters = filterSnapshots,
          comments = commentSnapshots,
          scoreThreshold = scoreThreshold,
        )
      )
    val commentContentMatches =
      evaluateGalleryCommentContentMatches(
        filters = filterSnapshots,
        comments = comments,
        evaluationFilterIndices = preflight.commentEvaluationFilterIndices,
      )
    val plan =
      planGalleryFilter(
        GalleryFilterRequest.comments(
          filters = filterSnapshots,
          comments = commentSnapshots,
          scoreThreshold = scoreThreshold,
          commentContentMatches = commentContentMatches,
        )
      )
    check(plan.keep.size == comments.size) { "Gallery comment filter plan length drifted" }
    return comments.filterIndexed { index, _ -> plan.keep[index] }
  }

  private suspend fun snapshots() =
    filters.await().map {
      GalleryFilterSnapshot(it.mode.field, it.text, it.enable)
    }
}

/** Executes Rust-ordered Android ICU work and returns an aligned sparse fact matrix. */
internal fun evaluateGalleryCommentContentMatches(
  filters: List<GalleryFilterSnapshot>,
  comments: List<GalleryComment>,
  evaluationFilterIndices: List<List<Int>>,
): List<List<Boolean?>> {
  require(evaluationFilterIndices.size == comments.size) {
    "Gallery comment evaluation plan length drifted"
  }
  return comments.mapIndexed { commentIndex, comment ->
    MutableList<Boolean?>(filters.size) { null }
      .also { facts ->
        for (filterIndex in evaluationFilterIndices[commentIndex]) {
          val filter = filters[filterIndex]
          val matched = androidIcuRegex(filter.text).containsMatchIn(comment.comment)
          facts[filterIndex] = matched
          if (matched) break
        }
      }
  }
}

private fun GalleryInfo.toFilterGallery() = GalleryFilterGallery(title, uploader, simpleTags)

private fun <T> MutableList<T>.applyKeepPlan(keep: List<Boolean>) {
  check(keep.size == size) { "Gallery filter plan length drifted" }
  val retained = filterIndexed { index, _ -> keep[index] }
  clear()
  addAll(retained)
}
