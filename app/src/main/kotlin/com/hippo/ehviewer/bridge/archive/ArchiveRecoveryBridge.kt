package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.download.RecoveryEntryInput
import com.hippo.ehviewer.bridge.download.RecoveryIssue
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

internal const val ARCHIVE_RECOVERY_VERSION = 3
internal const val ARCHIVE_RECOVERY_MAX_BATCH = 32

@Serializable
internal data class ArchiveRecoveryRequest(
  val version: Int,
  val startIndex: Int,
  val maxEntries: Int,
  val maximumEntries: Int,
  val allReadable: Boolean,
)

@Serializable
internal data class ArchiveRecoveryEntry(
  val name: String,
  val size: Long,
  val readable: Boolean,
) {
  fun toRecoveryEntry() = RecoveryEntryInput(name, size, regular = true, readable = readable)
}

@Serializable
internal data class ArchiveRecoveryBatch(
  val entryCount: Int,
  val nextIndex: Int,
  val allReadable: Boolean,
  val complete: Boolean,
  val issues: List<RecoveryIssue>,
  val entries: List<ArchiveRecoveryEntry>,
)

/** Exactly one V3 call. Validate the complete batch before applying any recovery facts. */
internal fun inspectArchiveRecovery(
  request: ArchiveRecoveryRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray,
): ArchiveRecoveryBatch {
  require(request.version == ARCHIVE_RECOVERY_VERSION)
  require(request.startIndex >= 0 && request.maxEntries in 1..ARCHIVE_RECOVERY_MAX_BATCH)
  require(request.maximumEntries > 0 && (request.startIndex != 0 || request.allReadable))
  val envelope =
    invokeRustCborBytes<ApiEnvelope<ArchiveRecoveryBatch>>(
      rustCbor.encodeToByteArray(request),
      "archive recovery batch",
      3 * 1024 * 1024,
      nativeCall,
    )
  require(envelope.version == ARCHIVE_RECOVERY_VERSION && envelope.module == "archiveRecovery") {
    "Unexpected archive recovery protocol"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val batch = checkNotNull(envelope.payload) { "Missing archive recovery batch" }
  require(batch.entryCount in 0..100_000 && request.startIndex <= batch.entryCount)
  require(batch.issues.distinct() == batch.issues)
  require(
    batch.issues.all {
      it in
        setOf(
          RecoveryIssue.ENCRYPTED_ARCHIVE,
          RecoveryIssue.LIMIT_EXCEEDED,
          RecoveryIssue.UNSAFE_ARCHIVE_PATH,
        )
    },
  )
  val encrypted = RecoveryIssue.ENCRYPTED_ARCHIVE in batch.issues
  val limited = RecoveryIssue.LIMIT_EXCEEDED in batch.issues
  require(encrypted == (batch.entryCount == 0))
  require(limited == (!encrypted && batch.entryCount > request.maximumEntries))
  require(!encrypted || request.startIndex == 0)
  val inspected = batch.entries.size
  require(inspected <= request.maxEntries && inspected <= batch.entryCount - request.startIndex)
  require(batch.nextIndex == request.startIndex + inspected)
  require((!encrypted && !limited && request.startIndex < batch.entryCount) == (inspected > 0))
  require(!encrypted && !limited || inspected == 0)
  require(
    batch.entries.all {
      it.name.isNotEmpty() &&
        it.name.encodeToByteArray().size <= 65_536 &&
        it.size in 1..512L * 1024 * 1024
    },
  )
  require(batch.allReadable == (request.allReadable && batch.entries.all { it.readable }))
  require(
    batch.complete ==
      (!encrypted && !limited && batch.nextIndex == batch.entryCount && batch.allReadable),
  )
  require(
    RecoveryIssue.UNSAFE_ARCHIVE_PATH !in batch.issues ||
      (!encrypted && !limited && batch.nextIndex == batch.entryCount),
  )
  return batch
}

/** Android owns scheduling/cancellation; every native result supplies the next inspection state. */
internal data class ArchiveRecoveryInspection(
  val entries: List<RecoveryEntryInput>,
  val complete: Boolean,
  val issues: List<RecoveryIssue>,
)

internal suspend fun collectArchiveRecoveryBatches(
  maximumEntries: Int,
  deadlineExceeded: () -> Boolean,
  inspect: (ArchiveRecoveryRequest) -> ArchiveRecoveryBatch,
): ArchiveRecoveryInspection {
  var request =
    ArchiveRecoveryRequest(
      ARCHIVE_RECOVERY_VERSION,
      0,
      ARCHIVE_RECOVERY_MAX_BATCH,
      maximumEntries,
      true,
    )
  val entries = ArrayList<RecoveryEntryInput>()
  var expectedCount: Int? = null
  while (true) {
    kotlinx.coroutines.currentCoroutineContext().ensureActive()
    if (deadlineExceeded()) {
      return ArchiveRecoveryInspection(entries, false, listOf(RecoveryIssue.LIMIT_EXCEEDED))
    }
    val batch = inspect(request)
    check(expectedCount == null || expectedCount == batch.entryCount) {
      "Archive recovery entry count changed during inspection"
    }
    expectedCount = batch.entryCount
    entries.addAll(batch.entries.map { it.toRecoveryEntry() })
    if (
      RecoveryIssue.ENCRYPTED_ARCHIVE in batch.issues ||
      RecoveryIssue.LIMIT_EXCEEDED in batch.issues
    ) {
      return ArchiveRecoveryInspection(entries, batch.complete, batch.issues)
    }
    if (batch.nextIndex == batch.entryCount) {
      kotlinx.coroutines.currentCoroutineContext().ensureActive()
      if (deadlineExceeded()) {
        return ArchiveRecoveryInspection(entries, false, listOf(RecoveryIssue.LIMIT_EXCEEDED))
      }
      return ArchiveRecoveryInspection(entries, batch.complete, batch.issues)
    }
    request = request.copy(startIndex = batch.nextIndex, allReadable = batch.allReadable)
  }
}
