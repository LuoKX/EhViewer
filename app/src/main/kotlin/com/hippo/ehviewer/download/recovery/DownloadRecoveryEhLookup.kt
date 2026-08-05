package com.hippo.ehviewer.download.recovery

import android.util.Log
import com.ehviewer.core.data.model.asEntity
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.EhDB
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
    val hints = candidate.lookupHints()
    val direct = lookupByKnownIdentity(candidate, hints.gid)
    if (direct != null) return RecoveryLookupResult.Choices(listOf(direct))

    val title = hints.title
    if (title.isNotBlank()) {
      val normalizedTitle = normalizeTitle(title)
      val phrase = title.replace('"', ' ').trim()
      val titleMatches =
        try {
          search("search:title:$normalizedTitle", "\"$phrase\"") { galleries ->
            val exactMatches =
              galleries.filter { normalizeTitle(it.title) == normalizedTitle }.ifEmpty { galleries }
            hints.gid?.let { gid -> exactMatches.filter { it.gid == gid } } ?: exactMatches
          }
        } catch (_: NoHitsFoundException) {
          emptyList()
        }
      val titleResult = titleMatches.toLookupResult()
      if (titleResult !== RecoveryLookupResult.NotFound) return titleResult
    }
    hints.gid?.let { gid ->
      val gidMatches =
        try {
          search("search:gid:$gid", "gid:$gid") { galleries -> galleries.filter { it.gid == gid } }
        } catch (_: NoHitsFoundException) {
          emptyList()
        }
      return gidMatches.toLookupResult()
    }
    return RecoveryLookupResult.NotFound
  }

  suspend fun apply(candidate: DownloadRecoveryCandidate, gallery: BaseGalleryInfo): Boolean {
    require(candidate.gid == null || candidate.gid == gallery.gid) {
      "Selected EH gallery does not match the known gallery identifier"
    }
    require(gallery.gid > 0 && TOKEN.matches(gallery.token)) { "EH returned an invalid identity" }
    val resolvedGallery = hydrate(gallery)
    val existingIssues = candidate.issues.split(',').filter(String::isNotBlank)
    val unresolvedLocalIssues = existingIssues.filterNot {
      it in IDENTITY_RESOLVED_ISSUES || it in LOOKUP_RESULT_ISSUES
    }
    val complete =
      unresolvedLocalIssues.isEmpty() &&
        candidate.localPageCount > 0 &&
        resolvedGallery.pages == candidate.localPageCount
    val path =
      recoveryPromotionPath(candidate.relativePath, candidate.sourceKind, resolvedGallery.gid)
    val status =
      when {
        !complete -> DownloadRecoveryStatus.CONTENT_INCOMPLETE
        path == null -> DownloadRecoveryStatus.PATH_CONFLICT
        else -> DownloadRecoveryStatus.READY
      }
    val issues = buildList {
      addAll(unresolvedLocalIssues)
      if (resolvedGallery.pages != candidate.localPageCount) add("PAGE_COUNT_CONFLICT")
      if (path == null) add("ANDROID_PATH_CONFLICT")
    }
      .distinct()
      .joinToString(",")
    val updated =
      candidate.copy(
        gid = resolvedGallery.gid,
        token = resolvedGallery.token,
        title = resolvedGallery.title,
        titleJpn = resolvedGallery.titleJpn,
        declaredPages = resolvedGallery.pages,
        evidence =
          (candidate.evidence.split(',').filter(String::isNotBlank) + "EH_LOOKUP")
            .distinct()
            .joinToString(","),
        issues = issues,
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
    markLookupFailed(candidate, failure?.toLookupFailureIssue() ?: "EH_QUERY_FAILED")
  }

  suspend fun markFailure(candidate: DownloadRecoveryCandidate, failure: Throwable) {
    Log.e("DownloadRecoveryEhLookup", "Recovery lookup/apply failed", failure)
    val latest =
      EhDB.getDownloadRecoveryCandidates().firstOrNull { it.candidateId == candidate.candidateId }
    val promotionFailure = failure.toPromotionFailure()
    val lookupWasApplied = latest?.evidence?.split(',')?.any { it == "EH_LOOKUP" } == true
    if (promotionFailure != null || lookupWasApplied) {
      val current = latest ?: candidate
      val resolvedFailure =
        promotionFailure
          ?: PromotionFailure(
            status =
              current.status.takeUnless { it == DownloadRecoveryStatus.READY }
                ?: DownloadRecoveryStatus.PATH_CONFLICT,
            issue = "LOCAL_PROMOTION_FAILED",
          )
      EhDB.putDownloadRecoveryCandidate(
        current.copy(
          status = resolvedFailure.status,
          issues =
            (current.issues.split(',').filter(String::isNotBlank).filterNot {
                it in LOOKUP_RESULT_ISSUES
              } + resolvedFailure.issue)
              .distinct()
              .joinToString(","),
          updatedAt = System.currentTimeMillis(),
        )
      )
    } else {
      markLookupFailed(candidate, failure.toLookupFailureIssue())
    }
  }

  suspend fun markNotFound(candidate: DownloadRecoveryCandidate) {
    markLookupFailed(candidate, "EH_NO_MATCH")
  }

  private suspend fun markLookupFailed(candidate: DownloadRecoveryCandidate, issue: String) {
    EhDB.putDownloadRecoveryCandidate(
      candidate.copy(
        status = DownloadRecoveryStatus.QUERY_FAILED,
        issues =
          (candidate.issues.split(',').filter(String::isNotBlank).filterNot {
              it in LOOKUP_RESULT_ISSUES
            } + issue)
            .distinct()
            .joinToString(","),
        updatedAt = System.currentTimeMillis(),
      )
    )
  }

  private suspend fun lookupByKnownIdentity(
    candidate: DownloadRecoveryCandidate,
    gid: Long?,
  ): BaseGalleryInfo? {
    gid ?: return null
    candidate.pageToken?.let { pageToken ->
      val page = candidate.pageIndex ?: 0
      val resolved =
        try {
          RecoverySearchScheduler.execute("gtoken:$gid:$page") {
              listOf(
                BaseGalleryInfo(
                  gid = gid,
                  token = EhEngine.getGalleryToken(gid, pageToken, page),
                )
              )
            }
            .singleOrNull()
            ?.takeIf { TOKEN.matches(it.token) }
        } catch (cancelled: CancellationException) {
          throw cancelled
        } catch (_: Exception) {
          null
        }
      if (resolved != null) return resolved
    }
    return candidate.token?.takeIf(TOKEN::matches)?.let { token ->
      BaseGalleryInfo(gid = gid, token = token)
    }
  }

  private fun DownloadRecoveryCandidate.lookupHints(): RecoveryLookupHints {
    val sourceName = displayName.trim().substringAfterLast('/')
    val sourceMatch = DOWNLOAD_SOURCE_NAME.matchEntire(sourceName)
    val inferredGid = sourceMatch?.groupValues?.get(1)?.toLongOrNull()?.takeIf { it > 0 }
    val gid = this.gid ?: inferredGid
    val storedTitle = title?.trim().orEmpty()
    val titleWithoutGeneratedPrefix = gid?.let { storedTitle.removePrefix("$it-").trim() }.orEmpty()
    val generatedTitle = sourceMatch?.groups?.get(2)?.value?.trim().orEmpty()
    val resolvedTitle =
      titleWithoutGeneratedPrefix.ifBlank { generatedTitle }.ifBlank { storedTitle }
    return RecoveryLookupHints(gid = gid, title = resolvedTitle)
  }

  private suspend fun search(
    normalizedKey: String,
    query: String,
    select: (List<BaseGalleryInfo>) -> List<BaseGalleryInfo>,
  ): List<BaseGalleryInfo> =
    RecoverySearchScheduler.execute(normalizedKey) {
      val url = ListUrlBuilder(keyword = query).build()
      select(EhEngine.getRecoveryGalleryList(url).galleryInfoList)
        .filter { it.gid > 0 && TOKEN.matches(it.token) }
        .distinctBy { it.gid to it.token }
        .take(25)
    }

  private suspend fun hydrate(gallery: BaseGalleryInfo): BaseGalleryInfo =
    RecoverySearchScheduler.execute("gdata:${gallery.gid}") {
        listOf(BaseGalleryInfo(gid = gallery.gid, token = gallery.token)).also { galleries ->
          EhEngine.fillGalleryListByApi(galleries)
        }
      }
      .single()

  private fun List<BaseGalleryInfo>.toLookupResult(): RecoveryLookupResult =
    if (isEmpty()) RecoveryLookupResult.NotFound else RecoveryLookupResult.Choices(this)

  private fun normalizeTitle(title: String?): String =
    title.orEmpty().lowercase().replace(WHITESPACE, " ").trim()

  private fun Throwable.toLookupFailureIssue(): String {
    val types = generateSequence(this) { it.cause }.take(8).map { it.javaClass.simpleName }.toSet()
    return when {
      "NotLoggedInException" in types -> "EH_AUTH_REQUIRED"
      "CloudflareBypassException" in types -> "EH_CLOUDFLARE_FAILED"
      "UnknownHostException" in types -> "EH_DNS_FAILED"
      "SocketTimeoutException" in types || "HttpTimeoutException" in types -> "EH_TIMEOUT"
      "ConnectException" in types || "IOException" in types -> "EH_NETWORK_FAILED"
      "ParseException" in types ||
        "SerializationException" in types ||
        "RustFailureException" in types -> "EH_PARSE_FAILED"
      "EhException" in types ||
        "IllegalArgumentException" in types ||
        "IllegalStateException" in types ||
        "NoSuchElementException" in types -> "EH_RESPONSE_FAILED"
      else -> "EH_QUERY_FAILED"
    }
  }

  private fun Throwable.toPromotionFailure(): PromotionFailure? {
    val messages = generateSequence(this) { it.cause }.take(8).mapNotNull { it.message }.toSet()
    return when {
      "Recovery candidate no longer exists" in messages ->
        PromotionFailure(DownloadRecoveryStatus.SOURCE_MISSING, "RECOVERY_SOURCE_MISSING")
      messages.any {
        it == "Recovery candidate changed before promotion" ||
          it == "Recovery candidate is not ready" ||
          it == "Recovery candidate identity changed before promotion" ||
          it == "Recovery candidate token changed before promotion"
      } -> PromotionFailure(DownloadRecoveryStatus.IDENTITY_CONFLICT, "IDENTITY_CONFLICT")
      messages.any {
        it == "Recovery candidate has no trusted page count" ||
          it == "Recovery candidate is not locally complete" ||
          it == "The formal gallery page count conflicts with the recovery candidate"
      } -> PromotionFailure(DownloadRecoveryStatus.CONTENT_INCOMPLETE, "PAGE_COUNT_CONFLICT")
      "Recovery source path changed before promotion" in messages ->
        PromotionFailure(DownloadRecoveryStatus.PATH_CONFLICT, "ANDROID_PATH_CONFLICT")
      messages.any {
        it == "A valid source mapping already exists for this gallery" ||
          it == "The recovery source is already mapped to a different gallery"
      } -> PromotionFailure(DownloadRecoveryStatus.PATH_CONFLICT, "EXISTING_MAPPING_CONFLICT")
      "An active download cannot be replaced by recovery" in messages ->
        PromotionFailure(DownloadRecoveryStatus.IDENTITY_CONFLICT, "ACTIVE_DOWNLOAD_CONFLICT")
      "The formal gallery identity conflicts with the recovery candidate" in messages ->
        PromotionFailure(DownloadRecoveryStatus.IDENTITY_CONFLICT, "IDENTITY_CONFLICT")
      "Promoted download could not be reloaded" in messages ->
        PromotionFailure(DownloadRecoveryStatus.PATH_CONFLICT, "LOCAL_PROMOTION_FAILED")
      else -> null
    }
  }

  private val TOKEN = Regex("[0-9a-f]{10}")
  private val WHITESPACE = Regex("\\s+")
  private val DOWNLOAD_SOURCE_NAME =
    Regex(
      "^([1-9]\\d{0,18})(?:-(.+)|\\.(?:cbz|zip|rar|7z|tar|tar\\.gz|tar\\.xz))$",
      RegexOption.IGNORE_CASE,
    )
  private val IDENTITY_RESOLVED_ISSUES =
    setOf("MISSING_GID", "MISSING_TOKEN", "UNKNOWN_PAGE_COUNT", "INVALID_METADATA")
  private val LOOKUP_RESULT_ISSUES =
    setOf(
      "EH_QUERY_FAILED",
      "EH_NO_MATCH",
      "EH_AUTH_REQUIRED",
      "EH_CLOUDFLARE_FAILED",
      "EH_DNS_FAILED",
      "EH_TIMEOUT",
      "EH_NETWORK_FAILED",
      "EH_PARSE_FAILED",
      "EH_RESPONSE_FAILED",
    )

  private data class RecoveryLookupHints(val gid: Long?, val title: String)

  private data class PromotionFailure(val status: DownloadRecoveryStatus, val issue: String)
}
