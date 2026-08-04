package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Pure search operation owned by the selected backend. */
@Serializable
internal enum class SearchRuleKind {
  @SerialName("normalizeQuery") NORMALIZE_QUERY,
  @SerialName("wrapTagKeyword") WRAP_TAG_KEYWORD,
}

/** Versioned sanitized input for one search rule. */
@Serializable
internal data class SearchRuleRequest(
  val version: Int,
  val kind: SearchRuleKind,
  val input: String,
  @SerialName("translatedPrefix") val translatedPrefix: String? = null,
  @SerialName("translatedTag") val translatedTag: String? = null,
) {
  init {
    val valid =
      when (kind) {
        SearchRuleKind.NORMALIZE_QUERY -> translatedPrefix == null && translatedTag == null
        SearchRuleKind.WRAP_TAG_KEYWORD -> (translatedPrefix == null) == (translatedTag == null)
      }
    require(valid) { "Invalid Search Rule request shape" }
  }

  companion object {
    fun normalizeQuery(input: String) =
      SearchRuleRequest(SEARCH_RULE_API_VERSION, SearchRuleKind.NORMALIZE_QUERY, input)

    fun wrapTagKeyword(
      input: String,
      translatedPrefix: String? = null,
      translatedTag: String? = null,
    ) =
      SearchRuleRequest(
        SEARCH_RULE_API_VERSION,
        SearchRuleKind.WRAP_TAG_KEYWORD,
        input,
        translatedPrefix,
        translatedTag,
      )
  }
}

/** Complete pure value returned for one search rule. */
@Serializable internal data class SearchRulePlan(val kind: SearchRuleKind, val value: String)

/** Preserves the legacy pure rule when the owning operation selected Kotlin. */
internal fun planLegacySearchRule(request: SearchRuleRequest): SearchRulePlan {
  val value =
    when (request.kind) {
      SearchRuleKind.NORMALIZE_QUERY -> request.input.trim().replace(LEGACY_WHITESPACE_REGEX, " ")
      SearchRuleKind.WRAP_TAG_KEYWORD -> {
        val tag = request.input.substringAfter(':')
        val prefix = request.input.dropLast(tag.length + 1)
        when {
          request.translatedPrefix != null ->
            "${request.translatedPrefix}：${requireNotNull(request.translatedTag)}"
          request.input.contains(' ') -> "$prefix:\"$tag$\""
          else -> "${request.input}$"
        }
      }
    }
  return SearchRulePlan(request.kind, value)
}

/** Calls Rust once and validates the versioned response envelope. */
internal fun planSearchRule(
  request: SearchRuleRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanSearchRuleV1,
): SearchRulePlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<SearchRulePlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Search Rule",
      nativeCall = nativeCall,
    )
  require(envelope.version == SEARCH_RULE_API_VERSION) {
    "Unsupported Search Rule API version"
  }
  require(envelope.module == SEARCH_RULE_MODULE) {
    "Unexpected Search Rule module"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing Search Rule payload" }
  require(plan.kind == request.kind) { "Search Rule kind mismatch" }
  return plan
}

/** Applies exactly one implementation for the backend captured by the caller. */
internal fun resolveSearchRule(request: SearchRuleRequest, backend: CoreBackend): String =
  when (backend) {
    CoreBackend.LEGACY -> planLegacySearchRule(request).value
    CoreBackend.RUST -> planSearchRule(request).value
  }

private external fun nativePlanSearchRuleV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val SEARCH_RULE_API_VERSION = 1
internal const val SEARCH_RULE_MODULE = "searchRule"
private val LEGACY_WHITESPACE_REGEX = Regex("\\s+")
