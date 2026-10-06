@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.text

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable internal data class TextSpan(val start: Int, val end: Int, val flags: Int)

@Serializable
internal data class TextSpansRequest(
  @EncodeDefault val version: Int = 1,
  val textLength: Int,
  val spans: List<TextSpan>,
)

/** One batch; comment text and Compose style objects never enter the native contract. */
internal fun planTextSpans(
  request: TextSpansRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanTextSpansV1,
): List<TextSpan> {
  require(request.textLength >= 0 && request.spans.size <= 100_000) { "Invalid text spans input" }
  val envelope =
    invokeRustCbor<ApiEnvelope<List<TextSpan>>>(
      rustCbor.encodeToByteArray(request),
      "Text Spans",
      nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "textSpans") { "Invalid Text Spans envelope" }
  envelope.error?.let { throw RustFailureException(it) }
  val spans = checkNotNull(envelope.payload) { "Missing Text Spans payload" }
  require(spans.size <= request.spans.size * 4) { "Invalid Text Spans count" }
  var previous: TextSpan? = null
  spans.forEach { span ->
    require(span.flags in listOf(1, 2, 4, 8)) { "Invalid Text Spans style" }
    require(span.start >= 0 && span.start < span.end && span.end <= request.textLength) {
      "Invalid Text Spans range"
    }
    previous?.let {
      require(span.flags > it.flags || (span.flags == it.flags && span.start > it.end)) {
        "Invalid Text Spans ordering"
      }
    }
    previous = span
  }
  return spans
}

private external fun nativePlanTextSpansV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
