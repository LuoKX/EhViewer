package com.hippo.ehviewer.spider

import com.hippo.ehviewer.bridge.archive.ComicInfoAction
import com.hippo.ehviewer.bridge.archive.ComicInfoFetchMode
import com.hippo.ehviewer.bridge.archive.ComicInfoMetadataState
import com.hippo.ehviewer.bridge.archive.ComicInfoPolicyPlan
import com.hippo.ehviewer.bridge.archive.ComicInfoPolicyRequest
import com.hippo.ehviewer.bridge.archive.planComicInfoPolicy
import kotlinx.coroutines.CancellationException

internal data class ObservedComicInfoPlan(val plan: ComicInfoPolicyPlan, val current: ComicInfo?)

/** Execute only Rust-requested reads. At most two calls; no reread/retry or local page policy. */
internal fun observeComicInfoPolicy(
  request: ComicInfoPolicyRequest,
  readMetadata: () -> ComicInfo?,
  planner: (ComicInfoPolicyRequest) -> ComicInfoPolicyPlan = { planComicInfoPolicy(it) },
): ObservedComicInfoPlan {
  require(request.metadataState == ComicInfoMetadataState.UNREAD && request.metadataPages == null)
  val initial = planner(request)
  if (initial.action != ComicInfoAction.READ_METADATA) return ObservedComicInfoPlan(initial, null)
  val current = readMetadata()
  val resolved =
    planner(
      request.copy(
        metadataState =
          if (current == null) ComicInfoMetadataState.MISSING else ComicInfoMetadataState.PRESENT,
        metadataPages = current?.pageCount,
      )
    )
  check(resolved.action != ComicInfoAction.READ_METADATA) { "Unresolved ComicInfo policy" }
  return ObservedComicInfoPlan(resolved, current)
}

/**
 * Interpret a validated Rust plan; networking, file writes and database effects stay Android-owned.
 */
internal suspend fun executeComicInfoPolicy(
  plan: ComicInfoPolicyPlan,
  fetchMetadata: suspend () -> Unit,
  logFetchFailure: (Exception) -> Unit,
  setGalleryPages: (Int) -> Unit,
  create: suspend () -> Unit,
  rewrite: (Int) -> Unit,
) {
  check(plan.action != ComicInfoAction.READ_METADATA) { "Unresolved ComicInfo policy" }
  if (plan.action == ComicInfoAction.SKIP) return
  when (plan.fetchMode) {
    ComicInfoFetchMode.NONE -> Unit
    ComicInfoFetchMode.PROPAGATE -> fetchMetadata()
    ComicInfoFetchMode.CONTINUE ->
      try {
        fetchMetadata()
      } catch (cancelled: CancellationException) {
        throw cancelled
      } catch (failure: Exception) {
        logFetchFailure(failure)
      }
  }
  plan.pageCount?.let(setGalleryPages)
  when (plan.action) {
    ComicInfoAction.CREATE -> create()
    ComicInfoAction.REWRITE -> rewrite(checkNotNull(plan.pageCount))
    ComicInfoAction.KEEP,
    ComicInfoAction.RESOLVE_PAGES,
    ComicInfoAction.SKIP -> Unit
    ComicInfoAction.READ_METADATA -> error("Unresolved ComicInfo policy")
  }
}
