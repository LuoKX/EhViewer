package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.bridge.text.*
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class BBCodeBridgeTest {
  @Test
  fun exportGoldenUsesExactlyOneNativeCallAndKeepsRawUtf16() {
    val request = rustCbor.decodeFromByteArray<BBCodeRequest>(golden("bbcode_export_request"))
    var calls = 0
    val result =
      planBBCode(request) { input, size, output ->
        calls++
        assertContentEquals(golden("bbcode_export_request"), read(input, size))
        respond(output, golden("bbcode_export_response"))
      }
    assertEquals(1, calls)
    assertEquals(
      "[b]中[i]😀[/i]\uD800x[/b]",
      result.units!!.map { it.toChar() }.toCharArray().concatToString(),
    )
    assertNull(result.edit)
  }

  @Test
  fun editGoldenUsesExactlyOneNativeCallAndReturnsExactSlices() {
    val request = rustCbor.decodeFromByteArray<BBCodeRequest>(golden("bbcode_edit_request"))
    var calls = 0
    val result =
      planBBCode(request) { input, size, output ->
        calls++
        assertContentEquals(golden("bbcode_edit_request"), read(input, size))
        respond(output, golden("bbcode_edit_response"))
      }
    assertEquals(1, calls)
    assertEquals(BBCodeEdit(1, 1, 1, 2), result.edit)
    assertNull(result.units)
  }

  @Test
  fun rejectsInvalidInputBeforeJni() {
    val request = export()
    val invalid =
      listOf(
        request.copy(version = 2),
        request.copy(units = listOf(-1)),
        request.copy(units = listOf(65536)),
        request.copy(units = List(65537) { 0 }),
        request.copy(originUnits = listOf(0)),
        request.copy(spans = List(100001) { TextSpan(0, 1, 1) }),
        request.copy(spans = listOf(TextSpan(-1, 1, 1))),
        request.copy(spans = listOf(TextSpan(1, 0, 1))),
        request.copy(spans = listOf(TextSpan(0, 2, 1))),
        request.copy(spans = listOf(TextSpan(0, 1, 16))),
        request.copy(operation = BBCodeOperation.EDIT),
        edit().copy(originUnits = List(65537) { 0 }),
        edit().copy(spans = listOf(TextSpan(0, 1, 1))),
      )
    invalid.forEach {
      assertFailsWith<IllegalArgumentException> {
        planBBCode(it) { _, _, _ -> fail("Invalid input entered JNI") }
      }
    }
  }

  @Test
  fun rejectsMalformedExportActionsAndUnits() {
    val good = BBCodePlan(BBCodeAction.EXPORT, listOf(97), null)
    val invalid =
      listOf(
        good.copy(action = BBCodeAction.TARGET),
        good.copy(edit = BBCodeEdit(0, 0, 0, 1)),
        good.copy(units = null),
        good.copy(units = emptyList()),
        good.copy(units = listOf(-1)),
        good.copy(units = listOf(65536)),
      )
    invalid.forEach {
      assertFailsWith<IllegalArgumentException> { planBBCode(export(), response(it)) }
    }
    assertFailsWith<IllegalArgumentException> {
      // Native response must fit transport capacity before adapter validation runs.
      planBBCode(
        export().copy(spans = List(30_000) { TextSpan(0, 1, 1) }),
        response(good.copy(units = List(1_000_001) { 0 })),
      )
    }
  }

  @Test
  fun rejectsMalformedEditActionsAndSlices() {
    val request = edit()
    val invalid =
      listOf(
        BBCodePlan(BBCodeAction.EXPORT, null, null),
        BBCodePlan(BBCodeAction.TARGET, listOf(97), null),
        BBCodePlan(BBCodeAction.ORIGINAL, null, null),
        BBCodePlan(BBCodeAction.TARGET, null, BBCodeEdit(0, 0, 0, 1)),
      )
    invalid.forEach {
      assertFailsWith<IllegalArgumentException> { planBBCode(request, response(it)) }
    }
    assertFailsWith<IllegalStateException> {
      planBBCode(request, response(BBCodePlan(BBCodeAction.SPLICE, null, null)))
    }
    val badSlices =
      listOf(
        BBCodeEdit(-1, 0, 0, 1),
        BBCodeEdit(0, 2, 0, 1),
        BBCodeEdit(0, 0, -1, 1),
        BBCodeEdit(0, 0, 0, 3),
        BBCodeEdit(0, 0, 1, 2),
        BBCodeEdit(0, 1, 0, 1),
        BBCodeEdit(0, 0, 0, 0),
        BBCodeEdit(0, 0, 0, 1),
      )
    badSlices.forEach {
      assertFailsWith<IllegalArgumentException> {
        planBBCode(request, response(BBCodePlan(BBCodeAction.SPLICE, null, it)))
      }
    }
  }

  @Test
  fun equalityTargetAndDeletionAreAcceptedWithoutFallback() {
    val request = edit()
    assertEquals(
      BBCodeAction.TARGET,
      planBBCode(request, response(BBCodePlan(BBCodeAction.TARGET, null, null))).action,
    )
    assertEquals(
      BBCodeAction.ORIGINAL,
      planBBCode(
          request.copy(units = request.originUnits!!),
          response(BBCodePlan(BBCodeAction.ORIGINAL, null, null)),
        )
        .action,
    )
    val deletion = request.copy(units = listOf(97), originUnits = listOf(97, 98))
    assertEquals(
      BBCodeEdit(1, 2, 1, 1),
      planBBCode(deletion, response(BBCodePlan(BBCodeAction.SPLICE, null, BBCodeEdit(1, 2, 1, 1))))
        .edit,
    )
  }

  @Test
  fun wrongEnvelopeAndMissingPayloadFail() {
    for ((version, module) in listOf(2 to "bbcode", 1 to "textSpans")) {
      assertFailsWith<IllegalArgumentException> {
        planBBCode(
          export(),
          envelope(ApiEnvelope(version, module, BBCodePlan(BBCodeAction.EXPORT, listOf(97), null))),
        )
      }
    }
    // Encode an invalid envelope directly: constructor also enforces this invariant.
    val invalid =
      "a46776657273696f6e01666d6f64756c65666262636f6465677061796c6f6164f6656572726f72f6"
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
    var calls = 0
    assertFailsWith<IllegalArgumentException> {
      planBBCode(export()) { _, _, output ->
        calls++
        respond(output, invalid)
      }
    }
    assertEquals(1, calls)
  }

  @Test
  fun typedRustFailureIsObservableWithoutRetry() {
    var calls = 0
    val bytes =
      rustCbor.encodeToByteArray(
        ApiEnvelope<BBCodePlan>(
          1,
          "bbcode",
          null,
          RustError(RustErrorKind.ERROR, "Invalid BBCode input"),
        )
      )
    assertFailsWith<RustFailureException> {
      planBBCode(export()) { _, _, output ->
        calls++
        respond(output, bytes)
      }
    }
    assertEquals(1, calls)
  }

  private fun export() =
    BBCodeRequest(
      operation = BBCodeOperation.EXPORT,
      units = listOf(97),
      originUnits = null,
      spans = emptyList(),
    )

  private fun edit() =
    BBCodeRequest(
      operation = BBCodeOperation.EDIT,
      units = listOf(97, 98),
      originUnits = listOf(97),
      spans = emptyList(),
    )

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun read(input: ByteBuffer, size: Int) =
    ByteArray(size).also { input.duplicate().get(it) }

  private fun respond(output: ByteBuffer, bytes: ByteArray): Int {
    output.put(bytes)
    return bytes.size
  }

  private fun response(plan: BBCodePlan) = envelope(ApiEnvelope(1, "bbcode", plan))

  private fun envelope(envelope: ApiEnvelope<BBCodePlan>): (ByteBuffer, Int, ByteBuffer) -> Int {
    val bytes = rustCbor.encodeToByteArray(envelope)
    return { _, _, output -> respond(output, bytes) }
  }
}
