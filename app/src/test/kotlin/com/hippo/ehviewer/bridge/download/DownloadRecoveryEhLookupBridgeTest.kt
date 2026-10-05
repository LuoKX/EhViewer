package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class DownloadRecoveryEhLookupBridgeTest {
  private fun fixture(name: String) =
    File("src/main/rust/crates/domain/test_data/recovery_eh_lookup_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  @Test
  fun exactRustGoldensAndOneCallPinLookupOrder() {
    val request =
      RecoveryEhRequest(
        1,
        RecoveryEhKind.LOOKUP,
        token = "012345abcd",
        title = "123- Name",
        displayName = "123- Name",
        hasPageToken = true,
      )
    assertEquals(fixture("request").toList(), rustCbor.encodeToByteArray(request).toList())
    var calls = 0
    val plan =
      planRecoveryEhLookup(request) { buffer, size, output ->
        calls++
        assertEquals(
          request,
          rustCbor.decodeFromByteArray<RecoveryEhRequest>(ByteArray(size).also(buffer::get)),
        )
        val bytes = fixture("response")
        output.put(bytes)
        bytes.size
      }
    assertEquals(123L, plan.gid)
    assertEquals("Name", plan.title)
    assertEquals(listOf("pageToken", "storedToken", "title", "gid"), plan.steps.map { it.kind })
    assertEquals(1, calls)
  }

  @Test
  fun invalidIndicesStatusesAndNativeFailuresNeverFallback() {
    val request =
      RecoveryEhRequest(
        1,
        RecoveryEhKind.SELECT,
        selection = RecoveryEhSelection.IDENTITY,
        candidates = listOf(RecoveryEhCandidate(1, "0000000001", null)),
      )
    val valid =
      RecoveryEhPlan(
        RecoveryEhKind.SELECT,
        null,
        null,
        emptyList(),
        listOf(0),
        null,
        null,
        null,
        false,
      )
    for (indices in listOf(listOf(-1), listOf(1), listOf(0, 0))) {
      assertFailsWith<IllegalArgumentException> {
        planRecoveryEhLookup(request) { _, _, output ->
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(1, "recoveryEhLookup", valid.copy(selected = indices))
            )
          output.put(bytes)
          bytes.size
        }
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      planRecoveryEhLookup(request) { _, _, _ -> throw UnsatisfiedLinkError("JNI") }
    }
    assertFailsWith<IllegalArgumentException> {
      planRecoveryEhLookup(request.copy(kind = RecoveryEhKind.APPLY)) { _, _, output ->
        val bytes =
          rustCbor.encodeToByteArray(
            ApiEnvelope(
              1,
              "recoveryEhLookup",
              valid.copy(
                kind = RecoveryEhKind.APPLY,
                status = "unknown",
                issues = "",
                evidence = "",
              ),
            )
          )
        output.put(bytes)
        bytes.size
      }
    }
  }
}
