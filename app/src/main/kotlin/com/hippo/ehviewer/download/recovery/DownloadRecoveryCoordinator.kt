package com.hippo.ehviewer.download.recovery

import android.content.Context
import android.net.Uri
import android.os.ParcelFileDescriptor
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoverySourceKind
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.database.model.GalleryEntity
import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.bridge.archive.RecoveryArchiveMetadataKind
import com.hippo.ehviewer.bridge.archive.RustArchiveReaderV2
import com.hippo.ehviewer.bridge.archive.collectArchiveRecoveryBatches
import com.hippo.ehviewer.bridge.download.DOWNLOAD_RECOVERY_API_VERSION
import com.hippo.ehviewer.bridge.download.ExistingRecoveryMapping
import com.hippo.ehviewer.bridge.download.RecoveryArchiveInput
import com.hippo.ehviewer.bridge.download.RecoveryCandidatePlan
import com.hippo.ehviewer.bridge.download.RecoveryCandidateRequest
import com.hippo.ehviewer.bridge.download.RecoveryConflictRequest
import com.hippo.ehviewer.bridge.download.RecoveryEntryInput
import com.hippo.ehviewer.bridge.download.RecoveryFingerprintInput
import com.hippo.ehviewer.bridge.download.RecoveryIssue
import com.hippo.ehviewer.bridge.download.RecoveryPageTokenEvidence
import com.hippo.ehviewer.bridge.download.RecoveryPersistenceAction
import com.hippo.ehviewer.bridge.download.RecoverySourceKeyInput
import com.hippo.ehviewer.bridge.download.RecoverySourceKind
import com.hippo.ehviewer.bridge.download.RecoveryStatus
import com.hippo.ehviewer.bridge.download.extractDownloadRecoveryPageToken
import com.hippo.ehviewer.bridge.download.inspectDownloadRecoveryCandidate
import com.hippo.ehviewer.bridge.download.planDownloadRecoveryConflicts
import com.hippo.ehviewer.bridge.files.RecoveryFileKind
import com.hippo.ehviewer.bridge.files.RecoveryFileObservation
import com.hippo.ehviewer.bridge.files.StorageCandidateInput
import com.hippo.ehviewer.bridge.files.StorageOperation
import com.hippo.ehviewer.bridge.files.StorageRulesRequest
import com.hippo.ehviewer.bridge.files.planStorageRules
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.download.DownloadManager
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileInputStream
import java.security.MessageDigest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive

internal data class RecoveryScanProgress(
  val discovered: Int = 0,
  val candidates: Int = 0,
  val processed: Int = 0,
  val recovered: Int = 0,
  val pending: Int = 0,
)

internal data class RecoveryScanResult(
  val progress: RecoveryScanProgress,
  val limitExceeded: Boolean,
)

internal class DownloadRecoveryCoordinator(
  private val context: Context,
) {
  private data class InspectedCandidate(
    val node: RecoveryDocumentNode,
    val roomKind: DownloadRecoverySourceKind,
    val plan: RecoveryCandidatePlan,
    val pageToken: RecoveryPageTokenEvidence?,
  )

  suspend fun scan(
    rootUri: Uri,
    runId: String,
    limits: RecoveryScanLimits,
    onProgress: suspend (RecoveryScanProgress) -> Unit = {},
  ): RecoveryScanResult {
    var progress = RecoveryScanProgress()
    val deadline = android.os.SystemClock.elapsedRealtime() + limits.maxDurationMillis
    val tree =
      SafTreeWalker.walk(context, rootUri, limits) { discovered ->
        progress = progress.copy(discovered = discovered)
        onProgress(progress)
      }
    val filesByParent =
      tree.nodes.asSequence().filterNot(RecoveryDocumentNode::isDirectory).groupBy {
        it.parentSourceId
      }
    val candidateFacts = ArrayList<StorageCandidateInput>()
    var candidateIdentityBytes = 0L
    var candidateObservationLimitExceeded = false
    classification@ for (chunk in tree.nodes
      .withIndex()
      .asSequence()
      .chunked(1024)) {
      currentCoroutineContext().ensureActive()
      val observations =
        chunk.map { (index, node) ->
          RecoveryFileObservation(
            index = index,
            directory = node.isDirectory,
            root = node.relativePath.isEmpty(),
            // Platform Regex observation preserves device case-folding; Rust owns eligibility.
            hasStrictPage =
              node.isDirectory &&
                filesByParent[node.sourceId].orEmpty().any {
                  STRICT_PAGE.matches(it.displayName)
                },
            suffixUtf16 = node.displayName.takeLast(7).toUtf16LeBytes(),
          )
        }
      val classified =
        planStorageRules(
          StorageRulesRequest(
            operation = StorageOperation.CLASSIFY,
            observations = observations,
          ),
        )
      for (candidate in classified.candidates) {
        val node = tree.nodes[candidate.index]
        candidateIdentityBytes += node.sourceId.length.toLong() * 2
        if (candidateIdentityBytes > 16 * 1024 * 1024) {
          candidateObservationLimitExceeded = true
          break@classification
        }
        candidateFacts +=
          StorageCandidateInput(candidate.index, candidate.kind, node.sourceId.toUtf16LeBytes())
      }
    }
    val candidatePlan =
      planStorageRules(
        StorageRulesRequest(
          operation = StorageOperation.CANDIDATES,
          candidates = candidateFacts,
          maxItems = limits.maxCandidates,
        ),
      )
    val allCandidates =
      candidatePlan.candidates.map {
        tree.nodes[it.index] to it.kind.toRoomKind()
      }
    val candidateLimitExceeded = candidateObservationLimitExceeded || candidatePlan.limitExceeded
    progress = progress.copy(candidates = allCandidates.size)
    onProgress(progress)

    val inspected = ArrayList<InspectedCandidate>(allCandidates.size)
    var inspectionLimitExceeded = false
    for ((node, roomKind) in allCandidates) {
      currentCoroutineContext().ensureActive()
      if (android.os.SystemClock.elapsedRealtime() >= deadline) {
        inspectionLimitExceeded = true
        break
      }
      val request =
        if (roomKind == DownloadRecoverySourceKind.DIRECTORY) {
          directoryRequest(tree.rootKey, node, filesByParent[node.sourceId].orEmpty())
        } else {
          archiveRequest(tree.rootKey, node, roomKind, limits, deadline)
        }
      val plan = inspectDownloadRecoveryCandidate(request)
      val pageToken =
        request.spiderInfo?.let { spiderInfo ->
          runCatching { extractDownloadRecoveryPageToken(spiderInfo) }.getOrNull()
        }
      inspected += InspectedCandidate(node, roomKind, plan, pageToken)
      progress = progress.copy(processed = inspected.size)
      onProgress(progress)
      if (android.os.SystemClock.elapsedRealtime() >= deadline) {
        inspectionLimitExceeded = true
        break
      }
    }

    val existingDownloads = EhDB.getAllDownloadInfo()
    val mappings =
      existingDownloads.map { download ->
        val matched =
          inspected.firstOrNull { candidate ->
            candidate.plan.status == RecoveryStatus.READY &&
              candidate.plan.gid == download.gid &&
              recoveryPromotionPath(
                candidate.node.relativePath,
                candidate.roomKind,
                candidate.plan.gid,
              ) == download.dirname
          }
        ExistingRecoveryMapping(
          gid = download.gid,
          sourceKey =
            matched?.plan?.sourceKey
              ?: sha256("database:${download.gid}:${download.dirname.orEmpty()}"),
          complete = download.state == DownloadInfo.STATE_FINISH,
        )
      }
    val decisions =
      planDownloadRecoveryConflicts(
        RecoveryConflictRequest(
          version = DOWNLOAD_RECOVERY_API_VERSION,
          candidates = inspected.map(InspectedCandidate::plan),
          existing = mappings,
        ),
      ).decisions
        .associateBy { it.sourceKey }
    val oldRows = EhDB.getDownloadRecoveryCandidates().associateBy { it.sourceKey }
    val now = System.currentTimeMillis()

    for (candidate in inspected) {
      currentCoroutineContext().ensureActive()
      val decision = requireNotNull(decisions[candidate.plan.sourceKey])
      if (decision.action == RecoveryPersistenceAction.KEEP_EXISTING) {
        oldRows[candidate.plan.sourceKey]?.let { old ->
          EhDB.removeDownloadRecoveryCandidate(old.candidateId)
        }
        continue
      }
      val plan =
        candidate.plan.copy(
          status = decision.status,
          action = decision.action,
          issues = (candidate.plan.issues + listOfNotNull(decision.issue)).distinct(),
        )
      val old = oldRows[plan.sourceKey]
      val promotionPath =
        recoveryPromotionPath(candidate.node.relativePath, candidate.roomKind, plan.gid)
      val pathBlocked =
        plan.action == RecoveryPersistenceAction.PROMOTE_FINISHED && promotionPath == null
      val row =
        DownloadRecoveryCandidate(
          candidateId = old?.candidateId ?: 0,
          sourceKey = plan.sourceKey,
          rootKey = tree.rootKey,
          sourceLocator = candidate.node.uri.toString(),
          relativePath = candidate.node.relativePath,
          displayName = candidate.node.displayName,
          sourceKind = candidate.roomKind,
          gid = plan.gid ?: old?.gid,
          token = plan.token ?: old?.token,
          pageToken = candidate.pageToken?.pageToken ?: old?.pageToken,
          pageIndex = candidate.pageToken?.pageIndex ?: old?.pageIndex,
          title = plan.title ?: old?.title,
          titleJpn = old?.titleJpn,
          declaredPages = plan.declaredPages ?: old?.declaredPages,
          localPageCount = plan.localPageCount,
          evidence = plan.evidence.joinToString(",") { it.kind.name },
          issues =
            (
              plan.issues.map { it.name } +
                if (pathBlocked) listOf("ANDROID_PATH_CONFLICT") else emptyList()
            ).distinct()
              .joinToString(","),
          status = if (pathBlocked) DownloadRecoveryStatus.PATH_CONFLICT else plan.toRoomStatus(),
          fingerprint = plan.fingerprint,
          lastSeenRunId = runId,
          createdAt = old?.createdAt ?: now,
          updatedAt = now,
        )
      EhDB.putDownloadRecoveryCandidate(row)
    }

    val staged = EhDB.getDownloadRecoveryCandidates().associateBy { it.sourceKey }
    var recovered = 0
    for (candidate in inspected) {
      currentCoroutineContext().ensureActive()
      val decision = requireNotNull(decisions[candidate.plan.sourceKey])
      if (
        decision.action != RecoveryPersistenceAction.PROMOTE_FINISHED ||
        decision.status != RecoveryStatus.READY
      ) {
        continue
      }
      val path =
        recoveryPromotionPath(candidate.node.relativePath, candidate.roomKind, candidate.plan.gid)
          ?: continue
      val row = staged[candidate.plan.sourceKey] ?: continue
      val gid = candidate.plan.gid ?: continue
      val token = candidate.plan.token ?: continue
      val pages = candidate.plan.declaredPages ?: candidate.plan.localPageCount
      val gallery =
        GalleryEntity(
          gid = gid,
          token = token,
          title = candidate.plan.title ?: candidate.node.displayName,
          titleJpn = null,
          thumbKey = null,
          category = 0,
          posted = null,
          uploader = null,
          rating = 0f,
          simpleTags = null,
          pages = pages,
          simpleLanguage = null,
          favoriteSlot = GalleryInfo.NOT_FAVORITED,
        )
      try {
        EhDB.promoteDownloadRecoveryCandidate(
          candidateId = row.candidateId,
          expectedSourceKey = row.sourceKey,
          gallery = gallery,
          relativePath = path,
          artists = emptyList(),
          promotedAt = now,
        )
        DownloadManager.reconcileVerifiedRecovery(gid)
        recovered++
      } catch (cancelled: CancellationException) {
        throw cancelled
      } catch (_: Throwable) {
        EhDB.putDownloadRecoveryCandidate(
          row.copy(
            status = DownloadRecoveryStatus.PATH_CONFLICT,
            issues =
              (row.issues.split(',').filter(String::isNotBlank) + "PROMOTION_CONFLICT")
                .distinct()
                .joinToString(","),
            updatedAt = System.currentTimeMillis(),
          ),
        )
      }
    }

    val limitExceeded = tree.limitExceeded || candidateLimitExceeded || inspectionLimitExceeded
    if (!limitExceeded) {
      EhDB.markDownloadRecoverySourcesMissing(tree.rootKey, runId, System.currentTimeMillis())
    }
    val pending =
      EhDB.getDownloadRecoveryCandidates().count {
        it.rootKey == tree.rootKey && it.status != DownloadRecoveryStatus.SOURCE_MISSING
      }
    progress = progress.copy(recovered = recovered, pending = pending)
    onProgress(progress)
    return RecoveryScanResult(progress, limitExceeded)
  }

  private fun directoryRequest(
    rootKey: String,
    directory: RecoveryDocumentNode,
    children: List<RecoveryDocumentNode>,
  ): RecoveryCandidateRequest {
    val ordered = children.sortedBy { it.sourceId }
    val spider = ordered.firstOrNull { it.displayName.equals(".ehviewer", true) }
    val comic = ordered.firstOrNull { it.displayName.equals("ComicInfo.xml", true) }
    return RecoveryCandidateRequest(
      version = DOWNLOAD_RECOVERY_API_VERSION,
      source = RecoverySourceKeyInput(rootKey, directory.sourceId, RecoverySourceKind.DIRECTORY),
      fingerprint =
        RecoveryFingerprintInput(
          size = ordered.sumOf { it.size ?: 0L },
          modifiedAtMillis = ordered.mapNotNull { it.modifiedAtMillis }.maxOrNull(),
        ),
      displayName = directory.displayName,
      spiderInfo = spider?.let { runCatching { readBounded(it) }.getOrNull() },
      comicInfo = comic?.let { runCatching { readBounded(it) }.getOrNull() },
      entries =
        ordered.map { child ->
          RecoveryEntryInput(
            name = child.displayName,
            size = child.size ?: 0,
            regular = !child.isDirectory,
            readable = child.size != 0L && canRead(child),
          )
        },
    )
  }

  private suspend fun archiveRequest(
    rootKey: String,
    node: RecoveryDocumentNode,
    roomKind: DownloadRecoverySourceKind,
    limits: RecoveryScanLimits,
    deadline: Long,
  ): RecoveryCandidateRequest {
    var spiderInfo: ByteArray? = null
    var comicInfo: ByteArray? = null
    var entries = emptyList<RecoveryEntryInput>()
    var archive =
      RecoveryArchiveInput(
        format = roomKind.name.lowercase(),
        complete = false,
      )
    try {
      openDescriptor(node).use { descriptor ->
        RustArchiveReaderV2.open(descriptor.fd, sortEntries = true).use { reader ->
          val inspection =
            collectArchiveRecoveryBatches(
              maximumEntries = limits.maxArchiveEntries,
              deadlineExceeded = { android.os.SystemClock.elapsedRealtime() >= deadline },
              inspect = reader::inspectRecovery,
            )
          entries = inspection.entries
          archive = archive.copy(issues = inspection.issues)
          if (
            RecoveryIssue.ENCRYPTED_ARCHIVE !in inspection.issues &&
            RecoveryIssue.LIMIT_EXCEEDED !in inspection.issues
          ) {
            currentCoroutineContext().ensureActive()
            spiderInfo = reader.recoveryMetadata(RecoveryArchiveMetadataKind.SPIDER_INFO)
            comicInfo = reader.recoveryMetadata(RecoveryArchiveMetadataKind.COMIC_INFO)
            archive = archive.copy(complete = inspection.complete)
          }
        }
      }
    } catch (cancelled: CancellationException) {
      throw cancelled
    } catch (failure: Throwable) {
      val limited = failure.message.orEmpty().contains("too many", ignoreCase = true)
      if (limited) {
        archive = archive.copy(issues = (archive.issues + RecoveryIssue.LIMIT_EXCEEDED).distinct())
      }
    }
    return RecoveryCandidateRequest(
      version = DOWNLOAD_RECOVERY_API_VERSION,
      source = RecoverySourceKeyInput(rootKey, node.sourceId, RecoverySourceKind.ARCHIVE),
      fingerprint = RecoveryFingerprintInput(node.size, node.modifiedAtMillis),
      displayName = node.displayName,
      spiderInfo = spiderInfo,
      comicInfo = comicInfo,
      entries = entries,
      archive = archive,
    )
  }

  private fun RecoveryCandidatePlan.toRoomStatus(): DownloadRecoveryStatus =
    when (status) {
      RecoveryStatus.READY -> {
        DownloadRecoveryStatus.READY
      }

      RecoveryStatus.MISSING_IDENTITY -> {
        DownloadRecoveryStatus.MISSING_IDENTITY
      }

      RecoveryStatus.INCOMPLETE -> {
        DownloadRecoveryStatus.CONTENT_INCOMPLETE
      }

      RecoveryStatus.UNREADABLE -> {
        DownloadRecoveryStatus.UNREADABLE
      }

      RecoveryStatus.LIMIT_EXCEEDED -> {
        DownloadRecoveryStatus.LIMIT_EXCEEDED
      }

      RecoveryStatus.SOURCE_MISSING -> {
        DownloadRecoveryStatus.SOURCE_MISSING
      }

      RecoveryStatus.CONFLICT -> {
        when {
          RecoveryIssue.DUPLICATE_SOURCE in issues -> DownloadRecoveryStatus.DUPLICATE_SOURCE
          RecoveryIssue.EXISTING_MAPPING_CONFLICT in issues -> DownloadRecoveryStatus.PATH_CONFLICT
          else -> DownloadRecoveryStatus.IDENTITY_CONFLICT
        }
      }
    }

  private fun readBounded(node: RecoveryDocumentNode): ByteArray =
    openInput(node).use { input ->
      val output = ByteArrayOutputStream()
      val buffer = ByteArray(16 * 1024)
      var remaining = MAX_METADATA_BYTES + 1
      while (remaining > 0) {
        val read = input.read(buffer, 0, minOf(buffer.size, remaining))
        if (read < 0) break
        output.write(buffer, 0, read)
        remaining -= read
      }
      output.toByteArray()
    }

  private fun canRead(node: RecoveryDocumentNode): Boolean =
    runCatching {
      openInput(node).use { it.read() >= 0 }
    }.getOrDefault(false)

  private fun openInput(node: RecoveryDocumentNode) =
    if (node.uri.scheme == "file") {
      FileInputStream(File(requireNotNull(node.uri.path)))
    } else {
      requireNotNull(context.contentResolver.openInputStream(node.uri))
    }

  private fun openDescriptor(node: RecoveryDocumentNode): ParcelFileDescriptor =
    if (node.uri.scheme == "file") {
      ParcelFileDescriptor.open(
        File(requireNotNull(node.uri.path)),
        ParcelFileDescriptor.MODE_READ_ONLY,
      )
    } else {
      requireNotNull(context.contentResolver.openFileDescriptor(node.uri, "r"))
    }

  private companion object {
    const val MAX_METADATA_BYTES = 512 * 1024
    val STRICT_PAGE =
      Regex(
        "^[0-9]{8}\\.(?:jpeg|jpg|png|gif|webp|bmp|ico|wbmp|heic|heif|avif)$",
        RegexOption.IGNORE_CASE,
      )
  }
}

internal fun recoveryPromotionPath(
  relativePath: String,
  kind: DownloadRecoverySourceKind,
  gid: Long?,
): String? {
  if (gid == null || gid <= 0) return null
  if (kind == DownloadRecoverySourceKind.DIRECTORY) return relativePath.takeIf(String::isNotBlank)
  if (kind != DownloadRecoverySourceKind.CBZ && kind != DownloadRecoverySourceKind.ZIP) return null
  val filename = relativePath.substringAfterLast('/')
  val expectedExtension = if (kind == DownloadRecoverySourceKind.CBZ) "cbz" else "zip"
  if (!filename.equals("$gid.$expectedExtension", ignoreCase = true)) return null
  return relativePath.substringBeforeLast('/', "").takeIf(String::isNotBlank)
}

private fun RecoveryFileKind.toRoomKind(): DownloadRecoverySourceKind =
  when (this) {
    RecoveryFileKind.DIRECTORY -> DownloadRecoverySourceKind.DIRECTORY
    RecoveryFileKind.CBZ -> DownloadRecoverySourceKind.CBZ
    RecoveryFileKind.ZIP -> DownloadRecoverySourceKind.ZIP
    RecoveryFileKind.RAR -> DownloadRecoverySourceKind.RAR
    RecoveryFileKind.SEVEN_ZIP -> DownloadRecoverySourceKind.SEVEN_ZIP
    RecoveryFileKind.TAR -> DownloadRecoverySourceKind.TAR
    RecoveryFileKind.TAR_GZ -> DownloadRecoverySourceKind.TAR_GZ
    RecoveryFileKind.TAR_XZ -> DownloadRecoverySourceKind.TAR_XZ
  }

private fun sha256(value: String): String =
  MessageDigest.getInstance("SHA-256").digest(value.encodeToByteArray()).joinToString("") {
    "%02x".format(it)
  }
