package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class SearchRuleBridgeTest {
  @Test
  fun legacyPlannerPreservesNormalizationAndTagFormatting() {
    assertEquals(
      "foo bar",
      planLegacySearchRule(SearchRuleRequest.normalizeQuery("\u2003 foo\t\nbar  \u2003")).value,
    )
    assertEquals(
      "artist:\"foo bar$\"",
      planLegacySearchRule(SearchRuleRequest.wrapTagKeyword("artist:foo bar")).value,
    )
    assertEquals(
      "艺术家：示例",
      planLegacySearchRule(SearchRuleRequest.wrapTagKeyword("artist:foo", "艺术家", "示例")).value,
    )
  }

  @Test
  fun selectedBackendIsExclusiveAndSelectedFailuresPropagate() {
    val request = SearchRuleRequest.normalizeQuery(" query ")
    assertEquals("query", resolveSearchRule(request, CoreBackend.LEGACY))
    assertFailsWith<LinkageError> {
      planSearchRule(request) { _, _, _ -> throw LinkageError("missing") }
    }
  }

  @Test
  fun nativeCallbackReceivesOneCompleteRequestAndReturnsItsPlan() {
    val request = SearchRuleRequest.wrapTagKeyword("artist:foo")
    val expected = SearchRulePlan(SearchRuleKind.WRAP_TAG_KEYWORD, "artist:foo$")
    var calls = 0
    val actual =
      planSearchRule(request) { input: ByteBuffer, size: Int, output: ByteBuffer ->
        calls += 1
        val bytes = ByteArray(size).also { input.get(it) }
        assertEquals(request, rustCbor.decodeFromByteArray<SearchRuleRequest>(bytes))
        val response =
          rustCbor.encodeToByteArray(
            ApiEnvelope(
              version = SEARCH_RULE_API_VERSION,
              module = SEARCH_RULE_MODULE,
              payload = expected,
            )
          )
        output.put(response)
        response.size
      }

    assertEquals(expected, actual)
    assertEquals(1, calls)
  }

  @Test
  fun bridgeRejectsMetadataAndOperationMismatches() {
    val request = SearchRuleRequest.normalizeQuery("query")
    val wrongKind = SearchRulePlan(SearchRuleKind.WRAP_TAG_KEYWORD, "query$")
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(request, ApiEnvelope(2, SEARCH_RULE_MODULE, wrongKind))
    }
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(request, ApiEnvelope(1, SEARCH_RULE_MODULE, wrongKind))
    }
  }

  private fun callWithEnvelope(
    request: SearchRuleRequest,
    envelope: ApiEnvelope<SearchRulePlan>,
  ) =
    planSearchRule(request) { _: ByteBuffer, _: Int, output: ByteBuffer ->
      val response = rustCbor.encodeToByteArray(envelope)
      output.put(response)
      response.size
    }
}
