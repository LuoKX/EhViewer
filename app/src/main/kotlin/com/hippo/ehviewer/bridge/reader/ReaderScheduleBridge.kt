package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Stable page states owned by `eh_domain::reader::ReaderPageState`. */
@Serializable
internal enum class ReaderPageState {
  @SerialName("queued") QUEUED,
  @SerialName("loading") LOADING,
  @SerialName("blocked") BLOCKED,
  @SerialName("ready") READY,
  @SerialName("error") ERROR,
}

/** Complete platform-neutral input for one Rust reader scheduling decision. */
@Serializable
internal data class ReaderScheduleSnapshot(
  val pageStates: List<ReaderPageState>,
  val previousIndex: Int? = null,
) {
  init {
    require(pageStates.isNotEmpty()) { "Reader Schedule snapshot must contain pages" }
    require(pageStates.size <= MAX_READER_PAGE_COUNT) {
      "Reader Schedule snapshot exceeds $MAX_READER_PAGE_COUNT pages"
    }
    require(previousIndex == null || previousIndex in pageStates.indices) {
      "Reader Schedule previous index is outside the snapshot"
    }
  }
}

/** Stable command discriminants accepted by the V1 Rust reader scheduler. */
@Serializable
internal enum class ReaderScheduleCommandKind {
  @SerialName("request") REQUEST,
  @SerialName("retry") RETRY,
  @SerialName("restart") RESTART,
}

/**
 * One atomic reader command sent to `eh_domain::reader`.
 *
 * Nullable fields form a tagged union keyed by [kind]. Factories reject malformed indices and
 * prefetch distances before the JNI boundary.
 */
@ConsistentCopyVisibility
@Serializable
internal data class ReaderScheduleCommand
private constructor(
  val kind: ReaderScheduleCommandKind,
  val index: Int? = null,
  val cached: Boolean? = null,
  val prefetchPageCount: Int? = null,
  val originalImage: Boolean? = null,
) {
  init {
    val shapeIsValid =
      when (kind) {
        ReaderScheduleCommandKind.REQUEST ->
          index != null && cached != null && prefetchPageCount != null && originalImage == null
        ReaderScheduleCommandKind.RETRY ->
          index != null && cached == null && prefetchPageCount == null && originalImage != null
        ReaderScheduleCommandKind.RESTART ->
          index == null && cached == null && prefetchPageCount == null && originalImage == null
      }
    require(shapeIsValid) { "Reader Schedule command fields do not match $kind" }
    index?.let { require(it >= 0) { "Reader Schedule index must not be negative" } }
    prefetchPageCount?.let {
      require(it in 0..MAX_READER_PREFETCH_PAGE_COUNT) {
        "Reader Schedule prefetch count is outside the supported range"
      }
    }
  }

  companion object {
    /** Displays or fetches [index], then plans directional prefetching. */
    fun request(index: Int, cached: Boolean, prefetchPageCount: Int) =
      ReaderScheduleCommand(
        kind = ReaderScheduleCommandKind.REQUEST,
        index = index,
        cached = cached,
        prefetchPageCount = prefetchPageCount,
      )

    /** Clears and force-fetches [index], optionally at original resolution. */
    fun retry(index: Int, originalImage: Boolean) =
      ReaderScheduleCommand(
        kind = ReaderScheduleCommandKind.RETRY,
        index = index,
        originalImage = originalImage,
      )

    /** Clears the complete decoded-image cache and resets all page states. */
    fun restart() = ReaderScheduleCommand(ReaderScheduleCommandKind.RESTART)
  }
}

/** Stable ordered action discriminants emitted by the V1 Rust reader scheduler. */
@Serializable
internal enum class ReaderScheduleActionKind {
  @SerialName("setPrevious") SET_PREVIOUS,
  @SerialName("resetOne") RESET_ONE,
  @SerialName("useCached") USE_CACHED,
  @SerialName("resetAll") RESET_ALL,
  @SerialName("removeCache") REMOVE_CACHE,
  @SerialName("clearCache") CLEAR_CACHE,
  @SerialName("fetch") FETCH,
  @SerialName("prefetch") PREFETCH,
}

/**
 * One ordered Rust decision which the Android `PageLoader` adapter applies exactly once.
 *
 * The nullable fields mirror the internally tagged Rust enum and are validated during CBOR decode.
 */
@Serializable
internal data class ReaderScheduleAction(
  val kind: ReaderScheduleActionKind,
  val index: Int? = null,
  val force: Boolean? = null,
  val originalImage: Boolean? = null,
  val pages: List<Int>? = null,
  val boundsStart: Int? = null,
  val boundsEnd: Int? = null,
) {
  init {
    val indexOnly =
      index != null &&
        force == null &&
        originalImage == null &&
        pages == null &&
        boundsStart == null &&
        boundsEnd == null
    val noFields =
      index == null &&
        force == null &&
        originalImage == null &&
        pages == null &&
        boundsStart == null &&
        boundsEnd == null
    val shapeIsValid =
      when (kind) {
        ReaderScheduleActionKind.SET_PREVIOUS,
        ReaderScheduleActionKind.RESET_ONE,
        ReaderScheduleActionKind.USE_CACHED,
        ReaderScheduleActionKind.REMOVE_CACHE -> indexOnly
        ReaderScheduleActionKind.RESET_ALL,
        ReaderScheduleActionKind.CLEAR_CACHE -> noFields
        ReaderScheduleActionKind.FETCH ->
          index != null &&
            force != null &&
            originalImage != null &&
            pages == null &&
            boundsStart == null &&
            boundsEnd == null
        ReaderScheduleActionKind.PREFETCH ->
          index == null &&
            force == null &&
            originalImage == null &&
            pages != null &&
            boundsStart != null &&
            boundsEnd != null
      }
    require(shapeIsValid) { "Reader Schedule action fields do not match $kind" }
    index?.let { require(it >= 0) { "Reader Schedule action index must not be negative" } }
  }
}

/** Ordered platform-neutral effects returned by the Rust reader scheduler. */
@Serializable internal data class ReaderSchedulePlan(val actions: List<ReaderScheduleAction>)

/** Versioned V1 request consumed by `eh_domain::reader`. */
@Serializable
internal data class ReaderScheduleRequest(
  val version: Int,
  val snapshot: ReaderScheduleSnapshot,
  val command: ReaderScheduleCommand,
)

/** Encodes one reader command, invokes Rust, and validates its complete ordered plan. */
internal fun planReaderSchedule(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanReaderScheduleV1,
): ReaderSchedulePlan {
  val request =
    rustCbor.encodeToByteArray(
      ReaderScheduleRequest(READER_SCHEDULE_API_VERSION, snapshot, command)
    )
  return invokeRustCbor<ApiEnvelope<ReaderSchedulePlan>>(
      request,
      operation = "Reader Schedule",
      nativeCall = nativeCall,
    )
    .toReaderSchedulePlan(snapshot, command)
}

/** Validates the V1 envelope and command-specific action sequence before any platform effect. */
internal fun ApiEnvelope<ReaderSchedulePlan>.toReaderSchedulePlan(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
): ReaderSchedulePlan {
  require(version == READER_SCHEDULE_API_VERSION) {
    "Unsupported Reader Schedule API version: $version"
  }
  require(module == READER_SCHEDULE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Reader Schedule response has no payload" }
    .also {
      it.validate(snapshot, command)
    }
}

private fun ReaderSchedulePlan.validate(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
) {
  when (command.kind) {
    ReaderScheduleCommandKind.REQUEST -> validateRequest(snapshot, command)
    ReaderScheduleCommandKind.RETRY -> validateRetry(snapshot, command)
    ReaderScheduleCommandKind.RESTART ->
      require(actions.map(ReaderScheduleAction::kind) == RESTART_ACTION_KINDS) {
        "Reader Schedule restart actions have an invalid order"
      }
  }
}

private fun ReaderSchedulePlan.validateRequest(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
) {
  val index = checkNotNull(command.index)
  require(index in snapshot.pageStates.indices) {
    "Reader Schedule request index is outside the snapshot"
  }
  val expectedKinds =
    if (command.cached == true) CACHED_REQUEST_ACTION_KINDS else REQUEST_ACTION_KINDS
  require(actions.map(ReaderScheduleAction::kind) == expectedKinds) {
    "Reader Schedule request actions have an invalid order"
  }
  require(actions.dropLast(1).all { it.index == index }) {
    "Reader Schedule request actions target different pages"
  }
  if (command.cached == false) {
    actions
      .first { it.kind == ReaderScheduleActionKind.FETCH }
      .let { fetch ->
        require(fetch.force == false && fetch.originalImage == false) {
          "Reader Schedule request fetch must use normal image options"
        }
      }
  }
  actions.last().validatePrefetch(snapshot, command)
}

private fun ReaderSchedulePlan.validateRetry(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
) {
  val index = checkNotNull(command.index)
  require(index in snapshot.pageStates.indices) {
    "Reader Schedule retry index is outside the snapshot"
  }
  require(actions.map(ReaderScheduleAction::kind) == RETRY_ACTION_KINDS) {
    "Reader Schedule retry actions have an invalid order"
  }
  require(actions.all { it.index == index }) {
    "Reader Schedule retry actions target different pages"
  }
  val fetch = actions.last()
  require(fetch.force == true && fetch.originalImage == command.originalImage) {
    "Reader Schedule retry fetch options do not match the command"
  }
}

private fun ReaderScheduleAction.validatePrefetch(
  snapshot: ReaderScheduleSnapshot,
  command: ReaderScheduleCommand,
) {
  val index = checkNotNull(command.index)
  val pageIndices = checkNotNull(pages)
  val prefetchCount = checkNotNull(command.prefetchPageCount)
  val start = checkNotNull(boundsStart)
  val end = checkNotNull(boundsEnd)
  val forward = snapshot.previousIndex == null || index >= snapshot.previousIndex
  val prefetchRange: IntProgression =
    if (forward) {
      index + 1..(index + prefetchCount).coerceAtMost(snapshot.pageStates.lastIndex)
    } else {
      index - 1 downTo (index - prefetchCount).coerceAtLeast(0)
    }
  val expectedPages = prefetchRange.filter { candidate ->
    snapshot.pageStates[candidate] == ReaderPageState.QUEUED ||
      snapshot.pageStates[candidate] == ReaderPageState.ERROR
  }
  require(pageIndices == expectedPages) {
    "Reader Schedule prefetch pages do not exactly match the legacy request window"
  }
  val expectedStart = if (prefetchRange.step > 0) prefetchRange.first else prefetchRange.last
  val expectedEnd = if (prefetchRange.step > 0) prefetchRange.last else prefetchRange.first
  require(start == expectedStart - 5 && end == expectedEnd + 5) {
    "Reader Schedule prefetch bounds do not exactly match the legacy request window"
  }
}

/** Invokes the V1 `eh_domain::reader` scheduler using separate direct buffers. */
private external fun nativePlanReaderScheduleV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private val REQUEST_ACTION_KINDS =
  listOf(
    ReaderScheduleActionKind.SET_PREVIOUS,
    ReaderScheduleActionKind.RESET_ONE,
    ReaderScheduleActionKind.FETCH,
    ReaderScheduleActionKind.PREFETCH,
  )
private val CACHED_REQUEST_ACTION_KINDS =
  listOf(
    ReaderScheduleActionKind.SET_PREVIOUS,
    ReaderScheduleActionKind.USE_CACHED,
    ReaderScheduleActionKind.PREFETCH,
  )
private val RETRY_ACTION_KINDS =
  listOf(
    ReaderScheduleActionKind.RESET_ONE,
    ReaderScheduleActionKind.REMOVE_CACHE,
    ReaderScheduleActionKind.FETCH,
  )
private val RESTART_ACTION_KINDS =
  listOf(ReaderScheduleActionKind.CLEAR_CACHE, ReaderScheduleActionKind.RESET_ALL)

private const val READER_SCHEDULE_API_VERSION = 1
private const val READER_SCHEDULE_MODULE = "readerSchedule"
private const val MAX_READER_PAGE_COUNT = 100_000
private const val MAX_READER_PREFETCH_PAGE_COUNT = 1_024
