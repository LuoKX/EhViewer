package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.bridge.files.*
import com.hippo.ehviewer.bridge.text.*
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class P3BridgeTest {
  @Test
  fun filenameGoldenRoutesOnceAndPreservesUtf16() {
    val request = rustCbor.decodeFromByteArray<FileRulesRequest>(golden("file_rules_request"))
    var calls = 0
    val result =
      planFileRules(
        request.kind,
        request.units!!.map { it.toChar() }.toCharArray().concatToString(),
      ) { buffer, size, output ->
        calls++
        assertContentEquals(golden("file_rules_request"), input(buffer, size))
        respond(output, golden("file_rules_response"))
      }
    assertEquals(1, calls)
    assertEquals("a__中😀", result)
    val lone = "\uD800"
    assertEquals(
      lone,
      planFileRules(
        FileRuleKind.BASENAME,
        lone,
        response(ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.BASENAME, listOf(0xD800)))),
      ),
    )
  }

  @Test
  fun rejectsFilenameEnvelopeOperationUnitsAndBudgetDrift() {
    for (envelope in
      listOf(
        ApiEnvelope(2, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(65))),
        ApiEnvelope(1, "wrong", FileRulesPlan(FileRuleKind.SANITIZE, listOf(65))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.BASENAME, listOf(65))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(-1))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(65536))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, emptyList())),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, List(201) { 65 })),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, List(100) { 0x800 })),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(47))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(92))),
        ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.SANITIZE, listOf(0))),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planFileRules(FileRuleKind.SANITIZE, "a", response(envelope))
      }
    }
  }

  @Test
  fun preservesEmptyNameExceptionAndSurfacesNativeFailuresWithoutRetry() {
    val failure =
      ApiEnvelope<FileRulesPlan>(
        1,
        "fileRules",
        error = RustError(RustErrorKind.ERROR, "Invalid filename"),
      )
    assertEquals(
      "Invalid filename",
      assertFailsWith<IllegalStateException> {
          planFileRules(FileRuleKind.SANITIZE, "", response(failure))
        }
        .message,
    )
    for (error in listOf(LinkageError("fixture"), IllegalStateException("fixture"))) {
      var calls = 0
      try {
        planFileRules(FileRuleKind.SANITIZE, "a") { _, _, _ ->
          calls++
          throw error
        }
        fail("Native failure hidden")
      } catch (actual: Throwable) {
        assertSame(error, actual)
      }
      assertEquals(1, calls)
    }
    assertFailsWith<RustFailureException> {
      planFileRules(
        FileRuleKind.SANITIZE,
        "a",
        response(
          ApiEnvelope<FileRulesPlan>(
            1,
            "fileRules",
            error = RustError(RustErrorKind.ERROR, "fixture"),
          )
        ),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      planFileRules(FileRuleKind.BASENAME, "a".repeat(65537)) { _, _, _ ->
        fail("Oversize entered JNI")
      }
    }
    assertNull(
      planFileRules(
        FileRuleKind.EXTENSION,
        null,
        response(ApiEnvelope(1, "fileRules", FileRulesPlan(FileRuleKind.EXTENSION, null))),
      )
    )
  }

  @Test
  fun spanGoldenBatchesEveryStyleInOneCall() {
    val request = rustCbor.decodeFromByteArray<TextSpansRequest>(golden("text_spans_request"))
    var calls = 0
    val result =
      planTextSpans(request) { buffer, size, output ->
        calls++
        assertContentEquals(golden("text_spans_request"), input(buffer, size))
        respond(output, golden("text_spans_response"))
      }
    assertEquals(1, calls)
    assertEquals(listOf(TextSpan(0, 6, 1), TextSpan(3, 6, 2)), result)
  }

  @Test
  fun rejectsSpanRangeOrderingStyleAndCountDriftBeforeUiEffects() {
    val request = TextSpansRequest(textLength = 20, spans = listOf(TextSpan(0, 10, 15)))
    for (spans in
      listOf(
        listOf(TextSpan(-1, 2, 1)),
        listOf(TextSpan(0, 21, 1)),
        listOf(TextSpan(0, 0, 1)),
        listOf(TextSpan(0, 1, 3)),
        listOf(TextSpan(0, 1, 16)),
        listOf(TextSpan(0, 2, 1), TextSpan(2, 4, 1)),
        listOf(TextSpan(0, 2, 2), TextSpan(3, 4, 1)),
        List(5) { TextSpan(it * 2, it * 2 + 1, 1) },
      )) {
      assertFailsWith<IllegalArgumentException> {
        planTextSpans(request, response(ApiEnvelope(1, "textSpans", spans)))
      }
    }
    for (envelope in
      listOf(
        ApiEnvelope(2, "textSpans", emptyList<TextSpan>()),
        ApiEnvelope(1, "wrong", emptyList<TextSpan>()),
      )) {
      assertFailsWith<IllegalArgumentException> { planTextSpans(request, response(envelope)) }
    }
    assertFailsWith<RustFailureException> {
      planTextSpans(
        request,
        response(
          ApiEnvelope<List<TextSpan>>(
            1,
            "textSpans",
            error = RustError(RustErrorKind.ERROR, "fixture"),
          )
        ),
      )
    }
    var calls = 0
    assertFailsWith<LinkageError> {
      planTextSpans(request) { _, _, _ ->
        calls++
        throw LinkageError("fixture")
      }
    }
    assertEquals(1, calls)
    assertFailsWith<IllegalArgumentException> {
      planTextSpans(request.copy(textLength = -1)) { _, _, _ -> fail("Invalid input entered JNI") }
    }
    assertFailsWith<IllegalArgumentException> {
      planTextSpans(request.copy(spans = List(100001) { TextSpan(0, 1, 1) })) { _, _, _ ->
        fail("Invalid input entered JNI")
      }
    }
  }

  private fun golden(name: String): ByteArray =
    File("src/main/rust/crates/domain/test_data/$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun input(buffer: ByteBuffer, size: Int) =
    ByteArray(size).also { buffer.duplicate().get(it) }

  private fun respond(output: ByteBuffer, bytes: ByteArray): Int {
    output.put(bytes)
    return bytes.size
  }

  private inline fun <reified T> response(
    envelope: ApiEnvelope<T>
  ): (ByteBuffer, Int, ByteBuffer) -> Int {
    val bytes = rustCbor.encodeToByteArray(envelope)
    return { _, _, output -> respond(output, bytes) }
  }
}
