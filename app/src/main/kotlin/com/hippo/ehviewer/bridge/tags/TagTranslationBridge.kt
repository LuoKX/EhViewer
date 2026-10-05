package com.hippo.ehviewer.bridge.tags

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 query of the immutable `eh_net::tag_translation` index. */
@Serializable
internal data class TagSuggestionRequest(
  val version: Int,
  val keyword: String,
  val translate: Boolean,
  val limit: Int,
)

/** V1 visible suggestion row; score retains Kotlin's UTF-16/Float semantics. */
@Serializable
internal data class TagSuggestion(val tag: String, val hint: String?, val score: Float)

/**
 * Resource operations owned by `eh_ffi::jvm::tag_translation`; callbacks support contract tests.
 */
internal class TagIndexNativeApi(
  val open: (Int, Int, Boolean) -> Long = ::nativeOpenTagIndexV1,
  val suggest: (Long, ByteBuffer, Int) -> ByteArray = ::nativeSuggestTagsV1,
  val translate: (Long, String, String) -> String? = ::nativeTranslateTagV1,
  val close: (Long) -> Unit = ::nativeCloseTagIndexV1,
)

/** Owns one native index. Caller holds its publication read lock during every operation. */
internal class TagTranslationIndex
private constructor(
  private val handle: Long,
  private val api: TagIndexNativeApi,
) : AutoCloseable {
  private var closed = false

  fun translate(prefix: String, tag: String): String? {
    check(!closed) { "Tag index is closed" }
    return api.translate(handle, prefix, tag)
  }

  fun suggest(request: TagSuggestionRequest): List<TagSuggestion> {
    check(!closed) { "Tag index is closed" }
    require(request.version == 1 && request.limit in 0..50) { "Invalid tag suggestion request" }
    val envelope =
      invokeRustCborBytes<ApiEnvelope<List<TagSuggestion>>>(
        rustCbor.encodeToByteArray(request),
        operation = "Tag suggestion",
        maxResponseSize = 4 * 1024 * 1024,
      ) { buffer, size ->
        api.suggest(handle, buffer, size)
      }
    require(envelope.version == 1 && envelope.module == "tagTranslation") {
      "Unexpected tag suggestion contract"
    }
    envelope.error?.let { throw RustFailureException(it) }
    val rows = checkNotNull(envelope.payload) { "Missing tag suggestion payload" }
    require(
      rows.size <= request.limit &&
        rows.all {
          it.tag.isNotEmpty() &&
            it.score.isFinite() &&
            it.score >= 0 &&
            (request.translate || it.hint == null)
        }
    ) {
      "Invalid tag suggestion rows"
    }
    return rows
  }

  override fun close() {
    if (!closed) {
      api.close(handle)
      closed = true
    }
  }

  companion object {
    /** Loads JSON through a descriptor. No complete JSON or dictionary enters the Java heap. */
    fun open(
      descriptor: Int,
      remote: Boolean,
      api: TagIndexNativeApi = TagIndexNativeApi(),
    ): TagTranslationIndex {
      val handle = api.open(1, descriptor, remote)
      check(handle > 0) { "Invalid tag index handle" }
      return TagTranslationIndex(handle, api)
    }
  }
}

/** V1 bounded descriptor snapshot loader owned by `eh_ffi`. */
private external fun nativeOpenTagIndexV1(version: Int, descriptor: Int, remote: Boolean): Long

/** V1 exact-sized CBOR query response owned by `eh_net`. */
private external fun nativeSuggestTagsV1(handle: Long, request: ByteBuffer, size: Int): ByteArray

/** V1 single indexed translation lookup owned by `eh_net`. */
private external fun nativeTranslateTagV1(handle: Long, prefix: String, tag: String): String?

/** V1 idempotent immutable-index retirement owned by `eh_ffi`. */
private external fun nativeCloseTagIndexV1(handle: Long)
