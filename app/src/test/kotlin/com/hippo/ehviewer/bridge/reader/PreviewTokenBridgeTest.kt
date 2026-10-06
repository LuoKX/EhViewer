package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class PreviewTokenBridgeTest {
  private val rows =
    listOf(
      PreviewToken(40, "first"),
      PreviewToken(40, ""),
      PreviewToken(-1, "negative"),
      PreviewToken(40, "last"),
    )
  private val request =
    PreviewTokenRequest(kind = PreviewTokenKind.APPLY, previewIndex = 2, previews = rows)
  private val payload =
    PreviewTokenPlan(PreviewTokenKind.APPLY, previewPerPage = 20, updates = listOf(0, 2, 3))

  @Test
  fun sharedGoldensPinCompleteRequestsAndEnvelopeFields() {
    val cases =
      listOf(
        "lookup" to
          PreviewTokenRequest(kind = PreviewTokenKind.LOOKUP, index = -7, previewPerPage = 2),
        "fallback" to
          PreviewTokenRequest(
            kind = PreviewTokenKind.LOOKUP,
            index = Int.MAX_VALUE,
            previewPerPage = 0,
          ),
        "apply" to request,
        "empty_first" to request.copy(previewIndex = 0, previews = emptyList()),
        "empty_nonzero" to request.copy(previewIndex = 1, previews = emptyList()),
        "overflow" to
          request.copy(previewIndex = -1, previews = listOf(PreviewToken(Int.MIN_VALUE, "opaque"))),
        "version" to
          PreviewTokenRequest(
            version = 2,
            kind = PreviewTokenKind.LOOKUP,
            index = 0,
            previewPerPage = 1,
          ),
      )
    for ((name, input) in cases) {
      assertContentEquals(golden(name, "request"), rustCbor.encodeToByteArray(input))
      assertEquals(
        input,
        rustCbor.decodeFromByteArray<PreviewTokenRequest>(golden(name, "request")),
      )
      val envelope =
        rustCbor.decodeFromByteArray<ApiEnvelope<PreviewTokenPlan>>(golden(name, "response"))
      assertEquals(1, envelope.version)
      assertEquals("previewTokens", envelope.module)
      if (name == "version") {
        assertEquals(
          RustError(RustErrorKind.ERROR, "Invalid preview token request"),
          envelope.error,
        )
        continue
      }
      var calls = 0
      if (envelope.error != null) {
        val failure =
          assertFailsWith<RustFailureException> {
            planPreviewTokens(input) { _, _ ->
              calls++
              golden(name, "response")
            }
          }
        assertFalse(failure.message.orEmpty().contains("opaque"))
      } else {
        assertContentEquals(
          golden(name, "plan"),
          rustCbor.encodeToByteArray(checkNotNull(envelope.payload)),
        )
        assertEquals(
          envelope.payload,
          rustCbor.decodeFromByteArray<PreviewTokenPlan>(golden(name, "plan")),
        )
        assertEquals(
          envelope.payload,
          planPreviewTokens(input) { buffer, size ->
            calls++
            assertTrue(buffer.isDirect)
            assertEquals(0, buffer.position())
            assertEquals(size, buffer.remaining())
            assertContentEquals(
              golden(name, "request"),
              ByteArray(size).also { buffer.duplicate().get(it) },
            )
            golden(name, "response")
          },
        )
      }
      assertEquals(1, calls)
    }
    assertEquals(
      payload,
      rustCbor
        .decodeFromByteArray<ApiEnvelope<PreviewTokenPlan>>(golden("apply", "response"))
        .payload,
    )
  }

  @Test
  fun rejectsWrongVersionShapesAndBudgetsBeforeNative() {
    val invalid =
      listOf(
        request.copy(version = 0),
        request.copy(index = 1),
        request.copy(previews = null),
        request.copy(previews = List(MAX_PREVIEW_TOKENS + 1) { PreviewToken(0, "") }),
        request.copy(previews = listOf(PreviewToken(0, "x".repeat(MAX_PREVIEW_TOKEN_BYTES + 1)))),
        request.copy(previews = List(17) { PreviewToken(0, "x".repeat(MAX_PREVIEW_TOKEN_BYTES)) }),
        request.copy(previews = listOf(PreviewToken(0, "中".repeat(22_000)))),
        request.copy(kind = PreviewTokenKind.LOOKUP),
        PreviewTokenRequest(kind = PreviewTokenKind.LOOKUP, index = 0),
      )
    for (input in invalid) {
      assertFailsWith<IllegalArgumentException> {
        planPreviewTokens(input) { _, _ -> error("Invalid request reached native") }
      }
    }
    // Count and token caps alone permit this, but encoded fact overhead exceeds 2 MiB.
    val wireOversize =
      request.copy(
        previews =
          List(MAX_PREVIEW_TOKENS) {
            PreviewToken(Int.MAX_VALUE, "")
          }
      )
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(wireOversize) { _, _ -> error("Oversize reached native") }
    }
  }

  @Test
  fun rejectsReorderedMissingRepeatedEmptyAndOutOfRangeDeltas() {
    for (updates in
      listOf(
        listOf(2, 0, 3),
        listOf(0, 3),
        listOf(0, 0, 3),
        listOf(0, 1, 3),
        listOf(-1, 2, 3),
        listOf(0, 2, 4),
        listOf(0, 2, Int.MAX_VALUE),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planPreviewTokens(request) { _, _ -> response(payload.copy(updates = updates)) }
      }
    }
    for (bad in
      listOf(
        payload.copy(previewIndex = 1),
        payload.copy(previewPerPage = null),
        payload.copy(updates = null),
        payload.copy(previewPerPage = 21),
        payload.copy(kind = PreviewTokenKind.LOOKUP),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planPreviewTokens(request) { _, _ -> response(bad) }
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(request.copy(previewIndex = 0)) { _, _ -> response(payload) }
    }
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(request.copy(previews = emptyList())) { _, _ ->
        response(payload.copy(updates = emptyList()))
      }
    }
  }

  @Test
  fun rejectsMalformedEnvelopeBytesModuleAndClosedEnum() {
    for (bytes in
      listOf(
        byteArrayOf(),
        byteArrayOf(0),
        ByteArray(MAX_PREVIEW_RESPONSE_BYTES + 1),
      )) {
      assertFailsWith<Exception> { planPreviewTokens(request) { _, _ -> bytes } }
    }
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(request) { _, _ -> response(payload, version = 2) }
    }
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(request) { _, _ -> response(payload, module = "spiderInfo") }
    }
    val unknown =
      response(payload)
        .toString(Charsets.ISO_8859_1)
        .replace("apply", "other")
        .toByteArray(Charsets.ISO_8859_1)
    assertFailsWith<Exception> { planPreviewTokens(request) { _, _ -> unknown } }
    val untrusted =
      rustCbor.encodeToByteArray(
        ApiEnvelope<PreviewTokenPlan>(
          1,
          "previewTokens",
          error = RustError(RustErrorKind.ERROR, "secret-token"),
        )
      )
    val failure =
      assertFailsWith<IllegalArgumentException> { planPreviewTokens(request) { _, _ -> untrusted } }
    assertFalse(failure.message.orEmpty().contains("secret-token"))
  }

  @Test
  fun validatesSignedQuotientWithoutReplacementPlanner() {
    for ((numerator, denominator, quotient) in
      listOf(
        Triple(-7, 2, -3),
        Triple(7, -2, -3),
        Triple(-7, -2, 3),
        Triple(Int.MIN_VALUE, -1, Int.MIN_VALUE),
      )) {
      val input =
        request.copy(
          previewIndex = denominator,
          previews = listOf(PreviewToken(numerator, "x")),
        )
      val output = payload.copy(previewPerPage = quotient, updates = listOf(0))
      assertEquals(output, planPreviewTokens(input) { _, _ -> response(output) })
      assertFailsWith<IllegalArgumentException> {
        planPreviewTokens(input) { _, _ ->
          response(output.copy(previewPerPage = quotient + 1))
        }
      }
    }
    val lookup = PreviewTokenRequest(kind = PreviewTokenKind.LOOKUP, index = -7, previewPerPage = 2)
    val output = PreviewTokenPlan(PreviewTokenKind.LOOKUP, previewIndex = -3)
    assertEquals(output, planPreviewTokens(lookup) { _, _ -> response(output) })
    for (bad in
      listOf(
        output.copy(previewIndex = -4),
        output.copy(updates = emptyList()),
        output.copy(previewPerPage = 1),
        output.copy(previewIndex = null),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planPreviewTokens(lookup) { _, _ -> response(bad) }
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planPreviewTokens(lookup.copy(previewPerPage = 0)) { _, _ -> response(output) }
    }
  }

  @Test
  fun nativeExceptionLinkageAndCancellationNeverFallback() {
    for (failure in
      listOf(
        IllegalStateException("native failure"),
        UnsatisfiedLinkError("missing export"),
        CancellationException("cancel"),
      )) {
      var calls = 0
      try {
        planPreviewTokens(request) { _, _ ->
          calls++
          throw failure
        }
        error("Failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(1, calls)
    }
  }

  private fun golden(
    name: String,
    direction: String,
  ): ByteArray =
    File("src/main/rust/crates/domain/test_data/preview_tokens_${name}_$direction.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map {
        it.toInt(16).toByte()
      }
      .toByteArray()

  private fun response(
    plan: PreviewTokenPlan,
    version: Int = 1,
    module: String = "previewTokens",
  ) = rustCbor.encodeToByteArray(ApiEnvelope(version, module, payload = plan))
}
