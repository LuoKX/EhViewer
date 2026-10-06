package com.hippo.ehviewer.download.recovery

import com.ehviewer.core.database.model.DownloadRecoverySourceKind
import com.hippo.ehviewer.bridge.download.DOWNLOAD_RECOVERY_API_VERSION
import com.hippo.ehviewer.bridge.download.RecoveryCandidateRequest
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryChild
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryOperation
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryPlan
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryPromotion
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryRequest
import com.hippo.ehviewer.bridge.download.RecoveryEntryInput
import com.hippo.ehviewer.bridge.download.RecoveryFingerprintInput
import com.hippo.ehviewer.bridge.download.RecoveryPathDecision
import com.hippo.ehviewer.bridge.download.RecoverySourceKeyInput
import com.hippo.ehviewer.bridge.download.RecoverySourceKind
import com.hippo.ehviewer.bridge.download.planRecoveryDirectory
import com.hippo.ehviewer.bridge.files.RecoveryFileKind
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes

/** Kotlin owns provider reads. Rust selects source slots before any read effect. */
internal fun observeRecoveryDirectory(
  rootKey: String,
  directory: RecoveryDocumentNode,
  children: List<RecoveryDocumentNode>,
  readBounded: (RecoveryDocumentNode) -> ByteArray,
  canRead: (RecoveryDocumentNode) -> Boolean,
  plan: (RecoveryDirectoryRequest) -> RecoveryDirectoryPlan = { planRecoveryDirectory(it) },
): RecoveryCandidateRequest {
  val observation =
    plan(
      RecoveryDirectoryRequest(
        1,
        RecoveryDirectoryOperation.OBSERVE,
        children.map {
          RecoveryDirectoryChild(
            it.sourceId.toUtf16LeBytes(),
            it.displayName.toUtf16LeBytes(),
            it.size,
            it.modifiedAtMillis,
          )
        },
        emptyList(),
      )
    )
  return RecoveryCandidateRequest(
    version = DOWNLOAD_RECOVERY_API_VERSION,
    source = RecoverySourceKeyInput(rootKey, directory.sourceId, RecoverySourceKind.DIRECTORY),
    fingerprint = RecoveryFingerprintInput(observation.totalSize, observation.modifiedAtMillis),
    displayName = directory.displayName,
    spiderInfo =
      observation.spiderIndex?.let {
        runCatching {
          readBounded(children[it])
        }
          .getOrNull()
      },
    comicInfo =
      observation.comicIndex?.let {
        runCatching {
          readBounded(children[it])
        }
          .getOrNull()
      },
    entries =
      observation.indices.map { index ->
        val child = children[index]
        RecoveryEntryInput(
          child.displayName,
          child.size ?: 0,
          !child.isDirectory,
          readable = child.size != 0L && canRead(child),
        )
      },
  )
}

internal data class RecoveryPromotionInput(
  val relativePath: String,
  val kind: DownloadRecoverySourceKind,
  val gid: Long?,
)

/** One complete scan batch, or one EH apply plan; no per-helper native calls during scan. */
internal fun recoveryPromotionPaths(
  sources: List<RecoveryPromotionInput>,
  plan: (RecoveryDirectoryRequest) -> RecoveryDirectoryPlan = { planRecoveryDirectory(it) },
): List<String?> {
  val request =
    RecoveryDirectoryRequest(
      1,
      RecoveryDirectoryOperation.PROMOTE,
      emptyList(),
      sources.map {
        RecoveryDirectoryPromotion(
          it.relativePath.toUtf16LeBytes(),
          // Room distinguishes RAR5; storage wire classifies both as RAR (ineligible).
          if (it.kind == DownloadRecoverySourceKind.RAR5) RecoveryFileKind.RAR
          else RecoveryFileKind.valueOf(it.kind.name),
          it.gid,
        )
      },
    )
  val result = plan(request)
  return result.promotions.map { path ->
    if (path.decision == RecoveryPathDecision.NONE) {
      null
    } else {
      sources[path.index].relativePath.substring(0, path.end)
    }
  }
}

/** EH lookup applies same Rust policy before Room promotion. */
internal fun recoveryPromotionPath(
  relativePath: String,
  kind: DownloadRecoverySourceKind,
  gid: Long?,
): String? =
  recoveryPromotionPaths(listOf(RecoveryPromotionInput(relativePath, kind, gid))).single()
