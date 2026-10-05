package com.hippo.ehviewer.download.recovery

import android.util.Log
import com.ehviewer.core.data.model.asEntity
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.bridge.download.RecoveryEhCandidate
import com.hippo.ehviewer.bridge.download.RecoveryEhKind
import com.hippo.ehviewer.bridge.download.RecoveryEhRequest
import com.hippo.ehviewer.bridge.download.RecoveryEhSelection
import com.hippo.ehviewer.bridge.download.RecoveryEhStep
import com.hippo.ehviewer.bridge.download.RecoveryPromotionSignal
import com.hippo.ehviewer.bridge.download.planRecoveryEhLookup
import com.hippo.ehviewer.client.EhEngine
import com.hippo.ehviewer.client.data.ListUrlBuilder
import com.hippo.ehviewer.client.exception.NoHitsFoundException
import com.hippo.ehviewer.download.DownloadManager
import kotlinx.coroutines.CancellationException

internal sealed interface RecoveryLookupResult {
  data class Choices(val galleries: List<BaseGalleryInfo>) : RecoveryLookupResult

  data object NotFound : RecoveryLookupResult
}

internal object DownloadRecoveryEhLookup {
  suspend fun lookup(candidate: DownloadRecoveryCandidate): RecoveryLookupResult {
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.LOOKUP,
          gid = candidate.gid,
          token = candidate.token,
          title = candidate.title,
          displayName = candidate.displayName,
          hasPageToken = candidate.pageToken != null,
          pageIndex = candidate.pageIndex ?: 0,
        )
      )
    for (step in plan.steps) {
      val choices =
        when (step.kind) {
          "pageToken" -> {
            val gid = checkNotNull(plan.gid)
            // Only EH/network failures advance to the next identity strategy. Native failures
            // from selection run outside this catch and must surface instead of masquerading as
            // misses.
            val observed =
              try {
                RecoverySearchScheduler.execute(step.key) {
                  listOf(
                    BaseGalleryInfo(
                      gid = gid,
                      token =
                        EhEngine.getGalleryToken(
                          gid,
                          checkNotNull(candidate.pageToken),
                          candidate.pageIndex ?: 0,
                        ),
                    )
                  )
                }
              } catch (cancelled: CancellationException) {
                throw cancelled
              } catch (_: Exception) {
                emptyList()
              }
            select(observed, RecoveryEhSelection.IDENTITY, plan.gid, plan.title)
          }
          "storedToken" ->
            listOf(
              BaseGalleryInfo(gid = checkNotNull(plan.gid), token = checkNotNull(candidate.token))
            )
          "title",
          "gid" -> {
            val observed =
              try {
                search(step)
              } catch (_: NoHitsFoundException) {
                emptyList()
              }
            select(
              observed,
              if (step.kind == "title") RecoveryEhSelection.TITLE else RecoveryEhSelection.GID,
              plan.gid,
              plan.title,
            )
          }
          else -> error("Unsupported EH recovery lookup step")
        }
      if (choices.isNotEmpty()) return RecoveryLookupResult.Choices(choices)
    }
    return RecoveryLookupResult.NotFound
  }

  suspend fun apply(candidate: DownloadRecoveryCandidate, gallery: BaseGalleryInfo): Boolean {
    planRecoveryEhLookup(
      RecoveryEhRequest(
        1,
        RecoveryEhKind.VALIDATE_IDENTITY,
        gid = gallery.gid,
        token = gallery.token,
        knownGid = candidate.gid,
      )
    )
    val resolvedGallery = hydrate(gallery)
    val path =
      recoveryPromotionPath(candidate.relativePath, candidate.sourceKind, resolvedGallery.gid)
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.APPLY,
          gid = resolvedGallery.gid,
          token = resolvedGallery.token,
          knownGid = candidate.gid,
          issues = candidate.issues,
          evidence = candidate.evidence,
          localPages = candidate.localPageCount,
          declaredPages = resolvedGallery.pages,
          usablePath = path != null,
        )
      )
    val status = DownloadRecoveryStatus.valueOf(checkNotNull(plan.status))
    val updated =
      candidate.copy(
        gid = resolvedGallery.gid,
        token = resolvedGallery.token,
        title = resolvedGallery.title,
        titleJpn = resolvedGallery.titleJpn,
        declaredPages = resolvedGallery.pages,
        evidence = checkNotNull(plan.evidence),
        issues = checkNotNull(plan.issues),
        status = status,
        updatedAt = System.currentTimeMillis(),
      )
    EhDB.putDownloadRecoveryCandidate(updated)
    if (status != DownloadRecoveryStatus.READY || path == null) return false
    EhDB.promoteDownloadRecoveryCandidate(
      candidateId = candidate.candidateId,
      expectedSourceKey = candidate.sourceKey,
      gallery = resolvedGallery.asEntity(),
      relativePath = path,
      artists =
        resolvedGallery.simpleTags.orEmpty().mapNotNull { tag ->
          tag.removePrefix("artist:").takeIf { it != tag && it.isNotBlank() }
        },
      promotedAt = System.currentTimeMillis(),
    )
    DownloadManager.reconcileVerifiedRecovery(resolvedGallery.gid)
    return true
  }

  suspend fun markQueryFailed(candidate: DownloadRecoveryCandidate, failure: Throwable? = null) {
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.QUERY_FAILURE,
          issues = candidate.issues,
          causeTypes = failure.causeTypes(),
        )
      )
    EhDB.putDownloadRecoveryCandidate(
      candidate.copy(
        status = DownloadRecoveryStatus.valueOf(checkNotNull(plan.status)),
        issues = checkNotNull(plan.issues),
        updatedAt = System.currentTimeMillis(),
      )
    )
  }

  suspend fun markFailure(candidate: DownloadRecoveryCandidate, failure: Throwable) {
    Log.e("DownloadRecoveryEhLookup", "Recovery lookup/apply failed", failure)
    val latest =
      EhDB.getDownloadRecoveryCandidates().firstOrNull { it.candidateId == candidate.candidateId }
    val current = latest ?: candidate
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.FAILURE,
          issues = candidate.issues,
          causeTypes = failure.causeTypes(),
          promotionSignals =
            generateSequence(failure) { it.cause }
              .take(8)
              .mapNotNull { PROMOTION_SIGNALS[it.message] }
              .distinct()
              .toList(),
          latestEvidence = latest?.evidence,
          latestIssues = latest?.issues,
          currentStatus = current.status.name,
        )
      )
    val target = if (plan.promotionFailure) current else candidate
    EhDB.putDownloadRecoveryCandidate(
      target.copy(
        status = DownloadRecoveryStatus.valueOf(checkNotNull(plan.status)),
        issues = checkNotNull(plan.issues),
        updatedAt = System.currentTimeMillis(),
      )
    )
  }

  suspend fun markNotFound(candidate: DownloadRecoveryCandidate) {
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.QUERY_FAILURE,
          issues = candidate.issues,
          notFound = true,
        )
      )
    EhDB.putDownloadRecoveryCandidate(
      candidate.copy(
        status = DownloadRecoveryStatus.valueOf(checkNotNull(plan.status)),
        issues = checkNotNull(plan.issues),
        updatedAt = System.currentTimeMillis(),
      )
    )
  }

  private fun select(
    galleries: List<BaseGalleryInfo>,
    selection: RecoveryEhSelection,
    gid: Long?,
    title: String?,
  ): List<BaseGalleryInfo> {
    val plan =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.SELECT,
          gid = gid,
          title = title,
          selection = selection,
          candidates = galleries.map { RecoveryEhCandidate(it.gid, it.token, it.title) },
        )
      )
    return plan.selected.map(galleries::get)
  }

  // Cache raw HTTP observations, never candidate-specific filtered decisions. The same normalized
  // title can be queried by candidates with different known gids; Rust selects separately each
  // time.
  private suspend fun search(step: RecoveryEhStep): List<BaseGalleryInfo> =
    RecoverySearchScheduler.execute(step.key) {
      EhEngine.getRecoveryGalleryList(ListUrlBuilder(keyword = checkNotNull(step.query)).build())
        .galleryInfoList
    }

  private suspend fun hydrate(gallery: BaseGalleryInfo): BaseGalleryInfo =
    RecoverySearchScheduler.execute("gdata:${gallery.gid}") {
        listOf(BaseGalleryInfo(gid = gallery.gid, token = gallery.token)).also {
          EhEngine.fillGalleryListByApi(it)
        }
      }
      .single()

  private fun Throwable?.causeTypes(): List<String> =
    generateSequence(this) { it.cause }.take(8).map { it.javaClass.simpleName }.toList()

  // Android Room exception strings become closed non-sensitive facts. Rust owns their precedence,
  // status/issue decisions and reconciliation; arbitrary Throwable messages never cross JNI.
  private val PROMOTION_SIGNALS =
    mapOf(
      "Recovery candidate no longer exists" to RecoveryPromotionSignal.SOURCE_MISSING,
      "Recovery candidate changed before promotion" to RecoveryPromotionSignal.IDENTITY_CONFLICT,
      "Recovery candidate is not ready" to RecoveryPromotionSignal.IDENTITY_CONFLICT,
      "Recovery candidate identity changed before promotion" to
        RecoveryPromotionSignal.IDENTITY_CONFLICT,
      "Recovery candidate token changed before promotion" to
        RecoveryPromotionSignal.IDENTITY_CONFLICT,
      "Recovery candidate has no trusted page count" to RecoveryPromotionSignal.PAGE_COUNT_CONFLICT,
      "Recovery candidate is not locally complete" to RecoveryPromotionSignal.PAGE_COUNT_CONFLICT,
      "The formal gallery page count conflicts with the recovery candidate" to
        RecoveryPromotionSignal.PAGE_COUNT_CONFLICT,
      "Recovery source path changed before promotion" to RecoveryPromotionSignal.PATH_CONFLICT,
      "A valid source mapping already exists for this gallery" to
        RecoveryPromotionSignal.MAPPING_CONFLICT,
      "The recovery source is already mapped to a different gallery" to
        RecoveryPromotionSignal.MAPPING_CONFLICT,
      "An active download cannot be replaced by recovery" to
        RecoveryPromotionSignal.ACTIVE_DOWNLOAD_CONFLICT,
      "The formal gallery identity conflicts with the recovery candidate" to
        RecoveryPromotionSignal.FORMAL_IDENTITY_CONFLICT,
      "Promoted download could not be reloaded" to RecoveryPromotionSignal.LOCAL_PROMOTION_FAILED,
    )
}
