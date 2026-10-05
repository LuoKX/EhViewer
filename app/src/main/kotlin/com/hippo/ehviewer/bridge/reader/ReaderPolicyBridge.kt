@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 closed operations owned by `eh_domain::reader_policy`. */
@Serializable
internal enum class ReaderPolicyKind {
  @SerialName("memory") MEMORY,
  @SerialName("pageTransition") PAGE_TRANSITION,
  @SerialName("ads") ADS,
  @SerialName("quota") QUOTA,
  @SerialName("speedLevel") SPEED_LEVEL,
}

/** V1 SpiderQueen state mapping; ordinal matches its closed legacy constants. */
@Serializable
internal enum class ReaderSpiderState {
  @SerialName("none") NONE,
  @SerialName("downloading") DOWNLOADING,
  @SerialName("finished") FINISHED,
  @SerialName("failed") FAILED,
}

/** V1 page listener effect. */
@Serializable
internal enum class ReaderPageNotification {
  @SerialName("none") NONE,
  @SerialName("success") SUCCESS,
  @SerialName("failure") FAILURE,
}

/** V1 gallery completion effect. */
@Serializable
internal enum class ReaderPageCompletion {
  @SerialName("none") NONE,
  @SerialName("archive") ARCHIVE,
  @SerialName("failure") FAILURE,
}

/** V1 platform observations. Credentials and exception objects never cross this boundary. */
@Serializable
internal data class ReaderPolicyRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val kind: ReaderPolicyKind,
  val appMaxMemory: Long? = null,
  val totalMemory: Long? = null,
  val pages: Int? = null,
  val finished: Int? = null,
  val downloaded: Int? = null,
  val oldState: ReaderSpiderState? = null,
  val newState: ReaderSpiderState? = null,
  val index: Int? = null,
  val enabled: Boolean? = null,
  val tags: List<String>? = null,
  val url: String? = null,
  val level: Int? = null,
)

/** V1 complete rule result. Only operation-specific fields may be present. */
@Serializable
internal data class ReaderPolicyPlan(
  val kind: ReaderPolicyKind,
  val cacheSize: Int? = null,
  val decodeConcurrency: Int? = null,
  val decodeAttempts: Int? = null,
  val finished: Int? = null,
  val downloaded: Int? = null,
  val notification: ReaderPageNotification? = null,
  val completion: ReaderPageCompletion? = null,
  val detect: Boolean? = null,
  val hasAds: Boolean? = null,
  val exceeded: Boolean? = null,
  val kibPerSecond: Int? = null,
)

/** Validates the whole V1 result before Android assigns counters, starts jobs or decodes. */
internal fun planReaderPolicy(
  request: ReaderPolicyRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanReaderPolicyV1,
): ReaderPolicyPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<ReaderPolicyPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Reader policy",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "readerPolicy") {
    "Unexpected reader policy contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing reader policy payload" }
  require(plan.kind == request.kind) { "Unexpected reader policy operation" }
  val fields =
    listOf(
      plan.cacheSize,
      plan.decodeConcurrency,
      plan.decodeAttempts,
      plan.finished,
      plan.downloaded,
      plan.notification,
      plan.completion,
      plan.detect,
      plan.hasAds,
      plan.exceeded,
      plan.kibPerSecond,
    )
  val required =
    when (plan.kind) {
      ReaderPolicyKind.MEMORY -> {
        require(
          plan.cacheSize != null &&
            plan.cacheSize in 1..128 * 1024 * 1024 &&
            plan.decodeConcurrency != null &&
            plan.decodeConcurrency in 1..2 &&
            plan.decodeAttempts != null &&
            plan.decodeAttempts in 1..2
        ) {
          "Invalid reader memory budget"
        }
        setOf(0, 1, 2)
      }
      ReaderPolicyKind.PAGE_TRANSITION -> {
        val pages = checkNotNull(request.pages)
        require(
          plan.finished != null &&
            plan.downloaded != null &&
            plan.finished in 0..plan.downloaded &&
            plan.downloaded <= pages &&
            plan.notification != null &&
            plan.completion != null
        ) {
          "Invalid reader page counters"
        }
        require(
          when (plan.completion) {
            ReaderPageCompletion.ARCHIVE -> plan.finished == pages && plan.downloaded == pages
            ReaderPageCompletion.FAILURE -> plan.finished < pages && plan.downloaded == pages
            ReaderPageCompletion.NONE -> plan.downloaded < pages
          }
        ) {
          "Invalid reader completion intent"
        }
        require(
          when (plan.notification) {
            ReaderPageNotification.SUCCESS -> request.newState == ReaderSpiderState.FINISHED
            ReaderPageNotification.FAILURE -> request.newState == ReaderSpiderState.FAILED
            ReaderPageNotification.NONE ->
              request.newState == ReaderSpiderState.NONE ||
                request.newState == ReaderSpiderState.DOWNLOADING
          }
        ) {
          "Invalid reader notification intent"
        }
        setOf(3, 4, 5, 6)
      }
      ReaderPolicyKind.ADS -> {
        require(plan.detect != null && plan.hasAds != null) { "Invalid reader advertisement plan" }
        setOf(7, 8)
      }
      ReaderPolicyKind.QUOTA -> {
        require(plan.exceeded != null) { "Invalid reader quota plan" }
        setOf(9)
      }
      ReaderPolicyKind.SPEED_LEVEL -> {
        require(plan.kibPerSecond != null && plan.kibPerSecond >= 0) { "Invalid speed threshold" }
        setOf(10)
      }
    }
  require(fields.indices.all { (fields[it] != null) == (it in required) }) {
    "Unexpected reader policy fields"
  }
  return plan
}

/** Scalar V1 sizing avoids CBOR/direct-buffer allocation in SieveCache insertion. */
internal fun readerImageEntrySize(
  allocationSize: Long,
  cacheSize: Int,
  nativeCall: (Int, Long, Int) -> Int = ::nativeReaderImageEntrySizeV1,
): Int =
  nativeCall(1, allocationSize, cacheSize).also {
    require(it in 1..cacheSize) { "Invalid reader cache entry size" }
  }

/** V1 `eh_domain::reader_policy` boundary. */
internal external fun nativePlanReaderPolicyV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int

private external fun nativeReaderImageEntrySizeV1(
  version: Int,
  allocationSize: Long,
  cacheSize: Int,
): Int
