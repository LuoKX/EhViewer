package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Stable lifecycle states owned by `eh_domain::download::DownloadState`. */
@Serializable
internal enum class DownloadState {
  @SerialName("invalid") INVALID,
  @SerialName("none") NONE,
  @SerialName("waiting") WAITING,
  @SerialName("downloading") DOWNLOADING,
  @SerialName("finished") FINISHED,
  @SerialName("failed") FAILED,
}

/** One positive gallery identifier and its download lifecycle state. */
@Serializable
internal data class DownloadEntry(@SerialName("gid") val galleryId: Long, val state: DownloadState)

/** Complete live queue state sent to the Rust reducer after process recovery. */
@Serializable
internal data class DownloadQueueSnapshot(
  val entries: List<DownloadEntry>,
  val waiting: List<Long>,
  val current: Long? = null,
)

/** Stable command discriminants accepted by the V1 Rust queue reducer. */
@Serializable
internal enum class DownloadCommandKind {
  @SerialName("startOne") START_ONE,
  @SerialName("startRange") START_RANGE,
  @SerialName("startAll") START_ALL,
  @SerialName("activateNext") ACTIVATE_NEXT,
  @SerialName("stopOne") STOP_ONE,
  @SerialName("stopRange") STOP_RANGE,
  @SerialName("stopAll") STOP_ALL,
  @SerialName("finish") FINISH,
}

/**
 * One atomic command sent to `eh_domain::download`.
 *
 * The nullable fields form a tagged union keyed by [kind]. Factory methods are the only supported
 * construction path and reject non-positive or duplicate IDs before JNI.
 */
@ConsistentCopyVisibility
@Serializable
internal data class DownloadQueueCommand
private constructor(
  val kind: DownloadCommandKind,
  @SerialName("gid") val galleryId: Long? = null,
  @SerialName("gids") val galleryIds: List<Long>? = null,
  val failed: Boolean? = null,
) {
  init {
    val shapeIsValid =
      when (kind) {
        DownloadCommandKind.START_ONE,
        DownloadCommandKind.STOP_ONE -> galleryId != null && galleryIds == null && failed == null
        DownloadCommandKind.START_RANGE -> galleryId == null && galleryIds != null && failed == null
        DownloadCommandKind.START_ALL,
        DownloadCommandKind.ACTIVATE_NEXT,
        DownloadCommandKind.STOP_ALL -> galleryId == null && galleryIds == null && failed == null
        DownloadCommandKind.STOP_RANGE -> galleryId == null && galleryIds != null && failed == null
        DownloadCommandKind.FINISH -> galleryId != null && galleryIds == null && failed != null
      }
    require(shapeIsValid) { "Download queue command fields do not match $kind" }
    galleryId?.let { require(it > 0) { "Gallery identifier must be positive" } }
    galleryIds?.let { ids ->
      require(ids.all { it > 0 }) { "Gallery identifiers must be positive" }
      require(ids.size == ids.toSet().size) { "Gallery identifiers must be unique" }
    }
  }

  companion object {
    /** Starts or retries one existing download. */
    fun startOne(galleryId: Long) = DownloadQueueCommand(DownloadCommandKind.START_ONE, galleryId)

    /** Starts eligible selected downloads in [galleryIds] order. */
    fun startRange(galleryIds: List<Long>) =
      DownloadQueueCommand(DownloadCommandKind.START_RANGE, galleryIds = galleryIds)

    /** Starts all idle or failed downloads in snapshot order. */
    fun startAll() = DownloadQueueCommand(DownloadCommandKind.START_ALL)

    /** Starts the next waiting download only when the worker is idle. */
    fun activateNext() = DownloadQueueCommand(DownloadCommandKind.ACTIVATE_NEXT)

    /** Stops one active or waiting download. */
    fun stopOne(galleryId: Long) = DownloadQueueCommand(DownloadCommandKind.STOP_ONE, galleryId)

    /** Stops selected active/waiting downloads, then advances once. */
    fun stopRange(galleryIds: List<Long>) =
      DownloadQueueCommand(DownloadCommandKind.STOP_RANGE, galleryIds = galleryIds)

    /** Stops the active worker and clears the FIFO. */
    fun stopAll() = DownloadQueueCommand(DownloadCommandKind.STOP_ALL)

    /** Records the terminal result of the active download. */
    fun finish(galleryId: Long, failed: Boolean) =
      DownloadQueueCommand(DownloadCommandKind.FINISH, galleryId, failed = failed)
  }
}

/** Versioned V1 request consumed by the Rust queue reducer. */
@Serializable
internal data class DownloadQueueRequest(
  val version: Int,
  val snapshot: DownloadQueueSnapshot,
  val command: DownloadQueueCommand,
)

/** Versioned persisted-entry request consumed by Rust after Android process death. */
@Serializable
internal data class DownloadQueueRecoveryRequest(
  val version: Int,
  val entries: List<DownloadEntry>,
)

/** Final state change for one gallery download. */
@Serializable
internal data class DownloadStateChange(
  @SerialName("gid") val galleryId: Long,
  val previous: DownloadState,
  val current: DownloadState,
)

/**
 * V1 Rust decision for one queue command.
 *
 * [startGalleryId] and [cancelGalleryId] are effects for Android's worker adapter. Rust never
 * starts services, opens paths, or writes Room rows.
 */
@Serializable
internal data class DownloadQueuePlan(
  val snapshot: DownloadQueueSnapshot,
  val changes: List<DownloadStateChange>,
  @SerialName("startGid") val startGalleryId: Long? = null,
  @SerialName("cancelGid") val cancelGalleryId: Long? = null,
)

/** Encodes one command, invokes Rust, and validates the returned V1 plan. */
internal fun reduceDownloadQueue(
  snapshot: DownloadQueueSnapshot,
  command: DownloadQueueCommand,
): DownloadQueuePlan {
  val request =
    rustCbor.encodeToByteArray(DownloadQueueRequest(DOWNLOAD_QUEUE_API_VERSION, snapshot, command))
  return invokeDownloadQueue(request, ::nativeReduceDownloadQueueV1).toDownloadQueuePlan(snapshot)
}

/** Normalizes persisted volatile states before rebuilding Android's in-memory queue. */
internal fun recoverDownloadQueue(entries: List<DownloadEntry>): DownloadQueuePlan {
  val request =
    rustCbor.encodeToByteArray(DownloadQueueRecoveryRequest(DOWNLOAD_QUEUE_API_VERSION, entries))
  return invokeDownloadQueue(request, ::nativeRecoverDownloadQueueV1)
    .toRecoveredDownloadQueuePlan(entries)
}

/** Invokes the selected JNI operation with owned request bytes and an atomic response buffer. */
internal fun invokeDownloadQueue(
  request: ByteArray,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): ApiEnvelope<DownloadQueuePlan> =
  invokeRustCbor(request, operation = "Download Queue", nativeCall = nativeCall)

/** Validates the V1 envelope and all structural queue/effect invariants. */
internal fun ApiEnvelope<DownloadQueuePlan>.toDownloadQueuePlan(
  previousSnapshot: DownloadQueueSnapshot? = null
): DownloadQueuePlan {
  require(version == DOWNLOAD_QUEUE_API_VERSION) {
    "Unsupported Download Queue API version: $version"
  }
  require(module == DOWNLOAD_QUEUE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Download Queue response has no payload" }
    .also {
      it.validate(previousSnapshot)
    }
}

/** Validates a V1 process-recovery envelope against its persisted input entries. */
internal fun ApiEnvelope<DownloadQueuePlan>.toRecoveredDownloadQueuePlan(
  previousEntries: List<DownloadEntry>
): DownloadQueuePlan = toDownloadQueuePlan().also { it.validateRecovery(previousEntries) }

private fun DownloadQueuePlan.validate(previousSnapshot: DownloadQueueSnapshot?) {
  val entries = snapshot.entries.associateBy(DownloadEntry::galleryId)
  require(entries.size == snapshot.entries.size) { "Download Queue entries must have unique IDs" }
  require(entries.keys.all { it > 0 }) { "Download Queue entry IDs must be positive" }

  val waiting = snapshot.waiting.toSet()
  require(waiting.size == snapshot.waiting.size && waiting.all { it > 0 }) {
    "Download Queue waiting IDs must be unique and positive"
  }
  require(waiting.all { entries[it]?.state == DownloadState.WAITING }) {
    "Download Queue FIFO must reference waiting entries"
  }
  require(
    snapshot.entries.filter { it.state == DownloadState.WAITING }.all { it.galleryId in waiting }
  ) {
    "Every waiting entry must be present in the Download Queue FIFO"
  }

  val downloading = snapshot.entries.filter { it.state == DownloadState.DOWNLOADING }
  if (snapshot.current == null) {
    require(downloading.isEmpty()) { "An idle Download Queue cannot contain an active entry" }
  } else {
    require(snapshot.current > 0 && downloading.singleOrNull()?.galleryId == snapshot.current) {
      "Download Queue current must identify its single active entry"
    }
  }

  require(changes.map(DownloadStateChange::galleryId).toSet().size == changes.size) {
    "Download Queue changes must have unique IDs"
  }
  changes.forEach { change ->
    require(change.galleryId > 0 && change.previous != change.current) {
      "Download Queue changes must be positive and observable"
    }
    require(entries[change.galleryId]?.state == change.current) {
      "Download Queue change must match the final snapshot"
    }
  }
  val changesById = changes.associateBy(DownloadStateChange::galleryId)
  startGalleryId?.let { galleryId ->
    require(galleryId == snapshot.current) {
      "Download Queue start effect must identify the final active entry"
    }
    require(changesById[galleryId]?.current == DownloadState.DOWNLOADING) {
      "Download Queue start effect must have a matching active-state change"
    }
  }
  cancelGalleryId?.let { galleryId ->
    require(galleryId > 0 && galleryId != startGalleryId) {
      "Download Queue cancel effect must identify a distinct positive entry"
    }
    require(entries[galleryId]?.state == DownloadState.NONE) {
      "Download Queue cancel effect must leave the entry idle"
    }
    require(
      changesById[galleryId]?.let { change ->
        change.previous == DownloadState.DOWNLOADING && change.current == DownloadState.NONE
      } == true
    ) {
      "Download Queue cancel effect must have a matching active-to-idle change"
    }
  }

  previousSnapshot?.let { previous ->
    require(
      previous.entries.map(DownloadEntry::galleryId) ==
        snapshot.entries.map(DownloadEntry::galleryId)
    ) {
      "Download Queue reducer must preserve entry identity and order"
    }
    val previousEntries = previous.entries.associateBy(DownloadEntry::galleryId)
    val expectedChanges =
      snapshot.entries.mapNotNull { entry ->
        previousEntries[entry.galleryId]?.let { before ->
          if (before.state == entry.state) null
          else DownloadStateChange(entry.galleryId, before.state, entry.state)
        }
      }
    require(changes == expectedChanges) {
      "Download Queue changes must exactly describe the request-to-response transition"
    }
  }
}

private fun DownloadQueuePlan.validateRecovery(previousEntries: List<DownloadEntry>) {
  require(
    snapshot.entries.map(DownloadEntry::galleryId) == previousEntries.map(DownloadEntry::galleryId)
  ) {
    "Download Queue recovery must preserve entry identity and order"
  }
  require(snapshot.waiting.isEmpty() && snapshot.current == null) {
    "Download Queue recovery must produce an idle empty queue"
  }
  require(startGalleryId == null && cancelGalleryId == null) {
    "Download Queue recovery cannot start or cancel platform workers"
  }
  val expectedEntries = previousEntries.map { entry ->
    entry.copy(
      state =
        when (entry.state) {
          DownloadState.WAITING,
          DownloadState.DOWNLOADING -> DownloadState.NONE
          else -> entry.state
        }
    )
  }
  require(snapshot.entries == expectedEntries) {
    "Download Queue recovery may only reset volatile states"
  }
  val expectedChanges = expectedEntries.mapNotNull { recovered ->
    previousEntries
      .first { it.galleryId == recovered.galleryId }
      .takeIf { it.state != recovered.state }
      ?.let { DownloadStateChange(recovered.galleryId, it.state, recovered.state) }
  }
  require(changes == expectedChanges) {
    "Download Queue recovery changes must exactly describe volatile-state normalization"
  }
}

/** Invokes the V1 `eh_domain::download` reducer using separate direct buffers. */
private external fun nativeReduceDownloadQueueV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

/** Invokes Rust process-restart recovery using separate direct buffers. */
private external fun nativeRecoverDownloadQueueV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private const val DOWNLOAD_QUEUE_API_VERSION = 1
private const val DOWNLOAD_QUEUE_MODULE = "downloadQueue"
