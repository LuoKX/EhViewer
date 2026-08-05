package com.hippo.ehviewer.bridge

import com.ehviewer.core.database.model.QuickSearch
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertSame
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class QuickSearchImportBridgeTest {
  @Test
  fun requestAndResponseCborGoldensMatchRustExactly() {
    val request =
      QuickSearchImportRequest.create(
        current = listOf(quickSearch(id = 10, name = "same", position = 0)),
        imported =
          listOf(
            quickSearch(id = 20, name = "same", position = 40),
            quickSearch(id = 21, name = "new", position = 41),
            quickSearch(id = 22, name = "new", position = 42),
          ),
      )
    assertEquals(REQUEST_GOLDEN, rustCbor.encodeToByteArray(request).toQuickSearchImportHex())

    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<QuickSearchImportPlan>>(
        RESPONSE_GOLDEN.decodeQuickSearchImportHex()
      )
    assertEquals(LIBRARY_RULE_API_VERSION, envelope.version)
    assertEquals(LIBRARY_RULE_MODULE, envelope.module)
    assertEquals(QuickSearchImportPlan(listOf(null, 1, 2)), envelope.payload)
    assertNull(envelope.error)
  }

  @Test
  fun nativePlanIsAppliedOnceInImportedSourceOrder() {
    val current = listOf(quickSearch(id = 10, name = "same", position = 0))
    val imported =
      listOf(
        quickSearch(id = 20, name = "same", position = 40),
        quickSearch(id = 21, name = "SAME", position = 41),
        quickSearch(id = 22, name = "new", position = 42),
        quickSearch(id = 23, name = "new", position = 43),
      )
    val expectedRequest = QuickSearchImportRequest.create(current, imported)
    var calls = 0

    val result =
      resolveQuickSearchImport(current, imported) { input, size, output ->
        calls += 1
        assertEquals(expectedRequest, input.decodeRequest<QuickSearchImportRequest>(size))
        output.writeEnvelope(QuickSearchImportPlan(listOf(null, 1, 2, 3)))
      }

    assertEquals(1, calls)
    assertEquals(listOf("SAME", "new", "new"), result.map(QuickSearch::name))
    assertEquals(listOf(1, 2, 3), result.map(QuickSearch::position))
    result.forEach { assertNull(it.id) }
    assertSame(imported[1], result[0])
    assertSame(imported[2], result[1])
    assertSame(imported[3], result[2])
    assertEquals(20L, imported[0].id)
    assertEquals(40, imported[0].position)
  }

  @Test
  fun malformedPlanFailsBeforeImportedRowsAreMutated() {
    val imported = listOf(quickSearch(id = 20, name = "new", position = 40))
    val before = imported.map { it.copy() }

    assertFailsWith<IllegalArgumentException> {
      resolveQuickSearchImport(emptyList(), imported) { _, _, output ->
        output.writeEnvelope(QuickSearchImportPlan(listOf(1)))
      }
    }

    assertEquals(before, imported)
  }

  @Test
  fun nativeFailurePropagatesWithoutFallbackOrMutation() {
    val imported = listOf(quickSearch(id = 20, name = "new", position = 40))
    val before = imported.map { it.copy() }
    val failure = LinkageError("missing QuickSearch import JNI")

    val actual =
      assertFailsWith<LinkageError> {
        resolveQuickSearchImport(emptyList(), imported) { _, _, _ -> throw failure }
      }

    assertSame(failure, actual)
    assertEquals(before, imported)
  }

  @Test
  fun positionValidationUsesJvmSignedIntWrapping() {
    val request =
      QuickSearchImportRequest(
        version = LIBRARY_RULE_API_VERSION,
        currentNames = emptyList(),
        importedNames = listOf("one", "two", "three"),
        currentCount = Int.MAX_VALUE,
      )

    assertEquals(
      QuickSearchImportPlan(listOf(Int.MAX_VALUE, Int.MIN_VALUE, Int.MIN_VALUE + 1)),
      planQuickSearchImport(request) { input, size, output ->
        assertEquals(request, input.decodeRequest<QuickSearchImportRequest>(size))
        output.writeEnvelope(
          QuickSearchImportPlan(listOf(Int.MAX_VALUE, Int.MIN_VALUE, Int.MIN_VALUE + 1))
        )
      },
    )
  }

  private inline fun <reified T> ByteBuffer.decodeRequest(size: Int): T =
    rustCbor.decodeFromByteArray(ByteArray(size).also(::get))

  private inline fun <reified T> ByteBuffer.writeEnvelope(payload: T): Int {
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(
          version = LIBRARY_RULE_API_VERSION,
          module = LIBRARY_RULE_MODULE,
          payload = payload,
        )
      )
    put(response)
    return response.size
  }

  private fun quickSearch(id: Long, name: String, position: Int) =
    QuickSearch(id = id, name = name, position = position)
}

private fun ByteArray.toQuickSearchImportHex() = joinToString("") { byte -> "%02x".format(byte) }

private fun String.decodeQuickSearchImportHex() =
  chunked(2).map { byte -> byte.toInt(16).toByte() }.toByteArray()

private const val REQUEST_GOLDEN =
  "a46776657273696f6e016c63757272656e744e616d6573816473616d656d696d706f727465644e616d6573836473616d65636e6577636e65776c63757272656e74436f756e7401"
private const val RESPONSE_GOLDEN =
  "a46776657273696f6e01666d6f64756c656b6c69627261727952756c65677061796c6f6164a169706f736974696f6e7383f60102656572726f72f6"
