package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Compact response facts; no URLs, gallery identity, or existing map. */
@Serializable
internal data class PreviewToken(
  val position: Int,
  val pToken: String,
)

@Serializable
internal enum class PreviewTokenKind {
  @SerialName("lookup") LOOKUP,
  @SerialName("apply") APPLY,
}

/** One call per HTTP page selection or complete response. */
@Serializable
internal data class PreviewTokenRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val kind: PreviewTokenKind,
  val index: Int? = null,
  val previewPerPage: Int? = null,
  val previewIndex: Int? = null,
  val previews: List<PreviewToken>? = null,
)

/** Sparse deltas reference immutable source facts; duplicate positions remain ordered. */
@Serializable
internal data class PreviewTokenPlan(
  val kind: PreviewTokenKind,
  val previewIndex: Int? = null,
  val previewPerPage: Int? = null,
  val updates: List<Int>? = null,
)

/** Validate entire result before any map, metadata, or HTTP effects. No fallback. */
internal fun planPreviewTokens(
  request: PreviewTokenRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativePlanPreviewTokensV1,
): PreviewTokenPlan {
  require(request.version == 1) { "Invalid preview token version" }
  require(
    when (request.kind) {
      PreviewTokenKind.LOOKUP -> {
        request.index != null &&
          request.previewPerPage != null &&
          request.previewIndex == null &&
          request.previews == null
      }

      PreviewTokenKind.APPLY -> {
        request.index == null &&
          request.previewPerPage == null &&
          request.previewIndex != null &&
          request.previews != null
      }
    }
  ) {
    "Invalid preview token request shape"
  }
  request.previews?.let { previews ->
    require(previews.size <= MAX_PREVIEW_TOKENS) { "Invalid preview token count" }
    var totalBytes = 0L
    previews.forEach {
      val count = it.pToken.toByteArray(Charsets.UTF_8).size
      require(count <= MAX_PREVIEW_TOKEN_BYTES) { "Invalid preview token length" }
      totalBytes += count
    }
    require(totalBytes <= MAX_PREVIEW_TOTAL_TOKEN_BYTES) { "Invalid preview token total length" }
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size <= MAX_PREVIEW_REQUEST_BYTES) { "Invalid preview token request size" }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<PreviewTokenPlan>>(
      encoded,
      operation = "preview tokens",
      maxResponseSize = MAX_PREVIEW_RESPONSE_BYTES,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "previewTokens") {
    "Unexpected preview token contract"
  }
  envelope.error?.let {
    require(
      it.kind == RustErrorKind.ERROR &&
        it.message in setOf("Invalid preview token request", "Empty nonzero preview page")
    ) {
      "Invalid preview token error"
    }
    throw RustFailureException(it)
  }
  val plan = checkNotNull(envelope.payload) { "Missing preview token payload" }
  require(plan.kind == request.kind) { "Unexpected preview token operation" }
  when (plan.kind) {
    PreviewTokenKind.LOOKUP -> {
      require(plan.previewIndex != null && plan.previewPerPage == null && plan.updates == null) {
        "Invalid preview lookup shape"
      }
      val size = checkNotNull(request.previewPerPage)
      if (size <= 0) {
        require(plan.previewIndex == 0) { "Invalid preview lookup fallback" }
      } else {
        validatePreviewQuotient(checkNotNull(request.index), size, plan.previewIndex)
      }
    }

    PreviewTokenKind.APPLY -> {
      require(plan.previewIndex == null && plan.previewPerPage != null && plan.updates != null) {
        "Invalid preview delta shape"
      }
      val previews = checkNotNull(request.previews)
      require(request.previewIndex == 0 || previews.isNotEmpty()) { "Invalid empty preview delta" }
      if (request.previewIndex == 0) {
        require(plan.previewPerPage == previews.size) { "Invalid first preview size" }
      } else {
        validatePreviewQuotient(
          previews.first().position,
          checkNotNull(request.previewIndex),
          plan.previewPerPage,
        )
      }
      require(
        plan.updates.size ==
          previews.count {
            it.pToken.isNotEmpty()
          }
      ) {
        "Incomplete preview delta"
      }
      var previous = -1
      plan.updates.forEach { index ->
        require(
          index > previous && index in previews.indices && previews[index].pToken.isNotEmpty()
        ) {
          "Invalid preview delta index"
        }
        previous = index
      }
    }
  }
  return plan
}

/** Verify returned quotient by remainder constraints, never compute a replacement page. */
private fun validatePreviewQuotient(numerator: Int, denominator: Int, quotient: Int) {
  if (numerator == Int.MIN_VALUE && denominator == -1) {
    require(quotient == Int.MIN_VALUE) { "Invalid preview quotient" }
    return
  }
  val remainder = numerator.toLong() - quotient.toLong() * denominator
  require(
    kotlin.math.abs(remainder) < kotlin.math.abs(denominator.toLong()) &&
      (remainder == 0L || (remainder > 0) == (numerator > 0))
  ) {
    "Invalid preview quotient"
  }
}

private external fun nativePlanPreviewTokensV1(
  request: ByteBuffer,
  size: Int,
): ByteArray

internal const val MAX_PREVIEW_TOKENS = 100_000
internal const val MAX_PREVIEW_TOKEN_BYTES = 65_536
internal const val MAX_PREVIEW_TOTAL_TOKEN_BYTES = 1024 * 1024
internal const val MAX_PREVIEW_REQUEST_BYTES = 2 * 1024 * 1024
internal const val MAX_PREVIEW_RESPONSE_BYTES = 1024 * 1024
