@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 directory observation consumed by `eh_domain::local_pages`. */
@Serializable
internal data class LocalPagesRequest(
  @kotlinx.serialization.EncodeDefault val version: Int = 1,
  val names: List<String>,
)

/** V1 immutable index/extension observation; filesystem paths stay Kotlin-owned. */
@Serializable internal data class LocalPageSelection(val sourceIndex: Int, val extension: String)

/** Rejects duplicate/out-of-range indices and mismatched extensions before opening any page. */
internal fun selectLocalPages(
  request: LocalPagesRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanLocalPagesV1,
): List<LocalPageSelection> {
  val envelope =
    invokeRustCbor<ApiEnvelope<List<LocalPageSelection>>>(
      rustCbor.encodeToByteArray(request),
      operation = "Local pages",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "localPages") {
    "Unexpected local pages contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val pages = checkNotNull(envelope.payload) { "Missing local pages payload" }
  val seen = HashSet<Int>()
  require(
    pages.all { page ->
      page.sourceIndex in request.names.indices &&
        seen.add(page.sourceIndex) &&
        page.extension.isNotEmpty() &&
        page.extension == request.names[page.sourceIndex].substringAfterLast('.', "")
    }
  ) {
    "Invalid local page selection"
  }
  return pages
}

/** V1 stable directory selection boundary. */
private external fun nativePlanLocalPagesV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int
