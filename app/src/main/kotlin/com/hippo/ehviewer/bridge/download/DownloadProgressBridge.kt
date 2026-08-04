package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

private const val DOWNLOAD_PROGRESS_API_VERSION = 1
private const val DOWNLOAD_PROGRESS_MODULE = "downloadProgress"

/** V1 immutable runtime counters owned by Rust progress planning; durations are milliseconds. */
@Serializable
internal data class DownloadProgressSnapshot(
  val state: DownloadState,
  @SerialName("speedBytesPerSecond") val speedBytesPerSecond: Long,
  @SerialName("remainingMilliseconds") val remainingMilliseconds: Long,
  val finished: Int,
  val downloaded: Int,
  val total: Int,
  val legacy: Int,
)

/** One active transfer observation; byte lengths are signed 64-bit values. */
@Serializable
internal data class ActiveDownloadTransfer(val contentLength: Long, val receivedSize: Long)

/** Command kind for runtime progress planning. */
@Serializable
internal enum class DownloadProgressCommandKind {
  @SerialName("reset") RESET,
  @SerialName("setPages") SET_PAGES,
  @SerialName("setPageResult") SET_PAGE_RESULT,
  @SerialName("estimate") ESTIMATE,
  @SerialName("finish") FINISH,
}

/** Tagged command with private factories enforcing field shape. */
@ConsistentCopyVisibility
@Serializable
internal data class DownloadProgressCommand
private constructor(
  val kind: DownloadProgressCommandKind,
  val finished: Int? = null,
  val downloaded: Int? = null,
  val total: Int? = null,
  @SerialName("speedBytesPerSecond") val speedBytesPerSecond: Long? = null,
  val transfers: List<ActiveDownloadTransfer>? = null,
  val failed: Boolean? = null,
) {
  init {
    val valid =
      when (kind) {
        DownloadProgressCommandKind.RESET ->
          total == null &&
            finished == null &&
            downloaded == null &&
            speedBytesPerSecond == null &&
            transfers == null &&
            failed == null
        DownloadProgressCommandKind.SET_PAGES ->
          total != null &&
            finished == null &&
            downloaded == null &&
            speedBytesPerSecond == null &&
            transfers == null &&
            failed == null
        DownloadProgressCommandKind.SET_PAGE_RESULT ->
          total != null &&
            finished != null &&
            downloaded != null &&
            speedBytesPerSecond == null &&
            transfers == null &&
            failed == null
        DownloadProgressCommandKind.ESTIMATE ->
          total == null &&
            finished == null &&
            downloaded == null &&
            speedBytesPerSecond != null &&
            transfers != null &&
            failed == null
        DownloadProgressCommandKind.FINISH ->
          total != null &&
            finished != null &&
            downloaded != null &&
            speedBytesPerSecond == null &&
            transfers == null &&
            failed != null
      }
    require(valid) { "Download progress command fields do not match $kind" }
  }

  companion object {
    /** Creates a reset command. */
    fun reset() = DownloadProgressCommand(kind = DownloadProgressCommandKind.RESET)

    /** Creates a page-count command. */
    fun setPages(total: Int) =
      DownloadProgressCommand(kind = DownloadProgressCommandKind.SET_PAGES, total = total)

    /** Creates a page-result command. */
    fun setPageResult(finished: Int, downloaded: Int, total: Int) =
      DownloadProgressCommand(
        kind = DownloadProgressCommandKind.SET_PAGE_RESULT,
        finished = finished,
        downloaded = downloaded,
        total = total,
      )

    /** Creates an ETA estimate command. */
    fun estimate(speedBytesPerSecond: Long, transfers: List<ActiveDownloadTransfer>) =
      DownloadProgressCommand(
        kind = DownloadProgressCommandKind.ESTIMATE,
        speedBytesPerSecond = speedBytesPerSecond,
        transfers = transfers,
      )

    /** Creates a terminal command. */
    fun finish(finished: Int, downloaded: Int, total: Int, failed: Boolean) =
      DownloadProgressCommand(
        kind = DownloadProgressCommandKind.FINISH,
        finished = finished,
        downloaded = downloaded,
        total = total,
        failed = failed,
      )
  }
}

/** Versioned request sent to the Rust `eh_domain::download::progress` owner. */
@Serializable
internal data class DownloadProgressRequest(
  val version: Int,
  val snapshot: DownloadProgressSnapshot,
  val command: DownloadProgressCommand,
)

/** Rust's immutable progress result. */
@Serializable internal data class DownloadProgressPlan(val snapshot: DownloadProgressSnapshot)

/** Calls Rust once and validates the versioned response envelope. */
internal fun planDownloadProgress(
  request: DownloadProgressRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanDownloadProgressV1,
): DownloadProgressPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<DownloadProgressPlan>>(
      rustCbor.encodeToByteArray(request),
      "Download Progress",
      nativeCall,
    )
  require(envelope.version == DOWNLOAD_PROGRESS_API_VERSION) {
    "Unsupported Download Progress API version"
  }
  require(envelope.module == DOWNLOAD_PROGRESS_MODULE) {
    "Unexpected Rust module: ${envelope.module}"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Missing Download Progress payload" }
}

/** JNI V1 entry point; Rust validates the request and atomically writes the envelope response. */
private external fun nativePlanDownloadProgressV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int
