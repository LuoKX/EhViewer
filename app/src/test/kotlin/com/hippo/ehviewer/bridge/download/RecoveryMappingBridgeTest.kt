package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class RecoveryMappingBridgeTest {
  private val request =
    RecoveryMappingRequest(
      1,
      listOf(
        RecoveryMappingCandidate(7, RecoveryStatus.READY, "a".repeat(64), "x".toUtf16LeBytes()),
        RecoveryMappingCandidate(7, RecoveryStatus.READY, "b".repeat(64), "x".toUtf16LeBytes()),
      ),
      listOf(RecoveryMappingDownload(7, "x".toUtf16LeBytes()), RecoveryMappingDownload(8, null)),
    )
  private val plan =
    RecoveryMappingPlan(
      listOf(
        RecoveryMapping(0, "a".repeat(64)),
        RecoveryMapping(null, "8fc478db3bc7cea0bfcd0fb1efde6ebf2f604aac0e2f855e94f39af9af80f811"),
      )
    )

  private fun golden(kind: String) =
    File("src/main/rust/crates/domain/test_data/recovery_mapping_$kind.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun response(value: RecoveryMappingPlan) =
    rustCbor.encodeToByteArray(ApiEnvelope(1, "recoveryMapping", value, null))

  @Test
  fun sharedWireAndExactlyOneNativeInvocation() {
    assertContentEquals(golden("request"), rustCbor.encodeToByteArray(request))
    assertContentEquals(golden("plan"), rustCbor.encodeToByteArray(plan))
    assertEquals(
      plan,
      rustCbor.decodeFromByteArray<ApiEnvelope<RecoveryMappingPlan>>(golden("response")).payload,
    )
    var calls = 0
    assertEquals(
      plan,
      planRecoveryMappings(request) { input, length, output ->
        calls++
        assertContentEquals(golden("request"), ByteArray(length).also { input.duplicate().get(it) })
        val bytes = golden("response")
        output.put(bytes)
        bytes.size
      },
    )
    assertEquals(1, calls)
  }

  @Test
  fun rejectsInputBudgetsBeforeNative() {
    val candidate = request.candidates[0]
    val invalid =
      listOf(
        request.copy(version = 2),
        request.copy(candidates = List(100_001) { candidate }),
        request.copy(downloads = List(100_001) { request.downloads[0] }),
        request.copy(candidates = listOf(candidate.copy(sourceKey = "secret-path"))),
        request.copy(candidates = listOf(candidate.copy(pathUtf16 = byteArrayOf(1)))),
        request.copy(downloads = listOf(RecoveryMappingDownload(8, byteArrayOf(1)))),
        request.copy(downloads = listOf(RecoveryMappingDownload(8, ByteArray(16 * 1024 * 1024)))),
      )
    invalid.forEach { input ->
      assertFailsWith<IllegalArgumentException> {
        planRecoveryMappings(input) { _, _, _ -> error("Invalid input reached native") }
      }
    }
  }

  @Test
  fun rejectsMalformedPlansBeforeConsumers() {
    val invalid =
      listOf(
        plan.copy(mappings = emptyList()),
        plan.copy(mappings = listOf(plan.mappings[0].copy(candidateIndex = -1), plan.mappings[1])),
        plan.copy(mappings = listOf(plan.mappings[0].copy(candidateIndex = 2), plan.mappings[1])),
        plan.copy(
          mappings = listOf(plan.mappings[0].copy(sourceKey = "b".repeat(64)), plan.mappings[1])
        ),
        plan.copy(
          mappings = listOf(plan.mappings[0], plan.mappings[1].copy(sourceKey = "G".repeat(64)))
        ),
        plan.copy(mappings = listOf(plan.mappings[0], plan.mappings[1].copy(candidateIndex = 0))),
      )
    invalid.forEach { value ->
      assertFailsWith<IllegalArgumentException> {
        planRecoveryMappings(request) { _, _, output ->
          response(value).also { output.put(it) }.size
        }
      }
    }
    for (candidate in
      listOf(
        request.candidates[0].copy(status = RecoveryStatus.CONFLICT),
        request.candidates[0].copy(gid = null),
        request.candidates[0].copy(pathUtf16 = null),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planRecoveryMappings(request.copy(candidates = listOf(candidate))) { _, _, output ->
          response(plan).also { output.put(it) }.size
        }
      }
    }
  }

  @Test
  fun failuresSurfaceWithoutReplay() {
    val linkage = UnsatisfiedLinkError("fixture")
    var calls = 0
    assertSame(
      linkage,
      assertFailsWith<UnsatisfiedLinkError> {
        planRecoveryMappings(request) { _, _, _ ->
          calls++
          throw linkage
        }
      },
    )
    assertEquals(1, calls)
    for (size in listOf(0, -1, Int.MAX_VALUE)) assertFailsWith<IllegalArgumentException> {
      planRecoveryMappings(request) { _, _, _ -> size }
    }
    assertFailsWith<IllegalArgumentException> {
      planRecoveryMappings(request) { _, _, output ->
        output.put(0xff.toByte())
        1
      }
    }
    for (bytes in
      listOf(
        rustCbor.encodeToByteArray(ApiEnvelope(2, "recoveryMapping", plan, null)),
        rustCbor.encodeToByteArray(ApiEnvelope(1, "wrong", plan, null)),
      )) assertFailsWith<IllegalArgumentException> {
      planRecoveryMappings(request) { _, _, output ->
        output.put(bytes)
        bytes.size
      }
    }
    val error =
      rustCbor.encodeToByteArray(
        ApiEnvelope<RecoveryMappingPlan>(
          1,
          "recoveryMapping",
          null,
          RustError(RustErrorKind.ERROR, "rejected"),
        )
      )
    assertFailsWith<RustFailureException> {
      planRecoveryMappings(request) { _, _, output ->
        output.put(error)
        error.size
      }
    }
  }
}
