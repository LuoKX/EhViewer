package com.hippo.ehviewer.bridge.files

import com.hippo.ehviewer.bridge.*
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class StorageRulesBridgeTest {
  @Test
  fun sharedGoldensMatchExactBytesAndOneNativeCallPerPlan() {
    for (name in listOf("classify", "candidates", "names", "cleanup")) {
      val input = golden("${name}_request")
      val request = rustCbor.decodeFromByteArray<StorageRulesRequest>(input)
      val output = golden("${name}_response")
      val expected = rustCbor.decodeFromByteArray<ApiEnvelope<StorageRulesPlan>>(output).payload!!
      var calls = 0
      val actual =
        planStorageRules(request) { buffer, size, response ->
          calls++
          assertTrue(buffer.isDirect && response.isDirect)
          assertContentEquals(input, ByteArray(size).also { buffer.duplicate().get(it) })
          response.put(output)
          output.size
        }
      assertEquals(expected, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun negativeCandidateLimitsKeepLegacyExceptionText() {
    val legacy = assertFailsWith<IllegalArgumentException> { emptyList<Int>().take(-1) }
    val actual =
      assertFailsWith<IllegalArgumentException> {
        planStorageRules(
          StorageRulesRequest(operation = StorageOperation.CANDIDATES, maxItems = -1)
        ) { _, _, _ ->
          fail("Negative candidate count entered JNI")
        }
      }
    assertEquals(legacy.message, actual.message)
  }

  @Test
  fun javaUtf16EncodingPreservesSurrogatesAndEndianOrder() {
    assertContentEquals(
      byteArrayOf(0, 0xd8.toByte(), 0, 0xdc.toByte(), 0, 1),
      "\uD800\uDC00\u0100".toUtf16LeBytes(),
    )
  }

  @Test
  fun malformedRequestsFailBeforeNativeEffects() {
    val names = StorageRulesRequest(operation = StorageOperation.NAMES)
    val bad =
      listOf(
        names.copy(version = 2),
        names.copy(maxItems = 1),
        names.copy(modified = listOf(1)),
        names.copy(names = listOf(StorageNameInput(byteArrayOf(1)))),
        names.copy(names = List(100_001) { StorageNameInput(byteArrayOf()) }),
        names.copy(names = listOf(StorageNameInput(ByteArray(16 * 1024 * 1024 + 2)))),
        StorageRulesRequest(operation = StorageOperation.CANDIDATES, maxItems = -1),
        StorageRulesRequest(
          operation = StorageOperation.CLASSIFY,
          observations = listOf(RecoveryFileObservation(-1, false, false, false, byteArrayOf())),
        ),
        StorageRulesRequest(
          operation = StorageOperation.CLASSIFY,
          observations = listOf(RecoveryFileObservation(0, false, false, false, ByteArray(16))),
        ),
        StorageRulesRequest(
          operation = StorageOperation.CLASSIFY,
          observations = listOf(RecoveryFileObservation(0, false, false, false, byteArrayOf(1))),
        ),
        StorageRulesRequest(
          operation = StorageOperation.CLASSIFY,
          observations =
            List(1025) { RecoveryFileObservation(it, false, false, false, byteArrayOf()) },
        ),
        StorageRulesRequest(
          operation = StorageOperation.CANDIDATES,
          candidates = listOf(StorageCandidateInput(100_000, RecoveryFileKind.ZIP, byteArrayOf())),
        ),
        StorageRulesRequest(
          operation = StorageOperation.CANDIDATES,
          candidates = List(2) { StorageCandidateInput(0, RecoveryFileKind.ZIP, byteArrayOf()) },
        ),
      )
    for (request in bad) assertFailsWith<IllegalArgumentException> {
      planStorageRules(request) { _, _, _ -> fail("Malformed request entered JNI") }
    }
  }

  @Test
  fun incompleteDuplicateAndOutOfRangeDeletionPlansAreRejected() {
    val request =
      StorageRulesRequest(
        operation = StorageOperation.CLEANUP,
        modified = listOf(3, 2, 1),
        maxItems = 1,
      )
    for (indices in listOf(listOf(1), listOf(1, 1), listOf(-1, 2), listOf(1, 3))) {
      assertFailsWith<IllegalArgumentException> {
        planStorageRules(request, success(StorageRulesPlan(indices, emptyList(), false)))
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planStorageRules(request, success(StorageRulesPlan(listOf(1, 2), emptyList(), true)))
    }
    assertFailsWith<IllegalArgumentException> {
      planStorageRules(
        request,
        success(
          StorageRulesPlan(
            listOf(1, 2),
            listOf(RecoveryCandidateRef(0, RecoveryFileKind.ZIP)),
            false,
          )
        ),
      )
    }
    assertEquals(
      listOf(1, 2),
      planStorageRules(request, success(StorageRulesPlan(listOf(1, 2), emptyList(), false)))
        .indices,
    )
  }

  @Test
  fun wrongCandidatesAndRootDirectoriesAreRejected() {
    val classified =
      StorageRulesRequest(
        operation = StorageOperation.CLASSIFY,
        observations = listOf(RecoveryFileObservation(0, true, true, true, byteArrayOf())),
      )
    for (candidate in
      listOf(
        RecoveryCandidateRef(0, RecoveryFileKind.DIRECTORY),
        RecoveryCandidateRef(0, RecoveryFileKind.ZIP),
        RecoveryCandidateRef(1, RecoveryFileKind.DIRECTORY),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planStorageRules(
          classified,
          success(StorageRulesPlan(emptyList(), listOf(candidate), false)),
        )
      }
    }
    val request =
      StorageRulesRequest(
        operation = StorageOperation.CANDIDATES,
        maxItems = 1,
        candidates = listOf(StorageCandidateInput(0, RecoveryFileKind.ZIP, "a".toUtf16LeBytes())),
      )
    for (plan in
      listOf(
        StorageRulesPlan(emptyList(), emptyList(), false),
        StorageRulesPlan(emptyList(), listOf(RecoveryCandidateRef(0, RecoveryFileKind.CBZ)), false),
        StorageRulesPlan(emptyList(), listOf(RecoveryCandidateRef(0, RecoveryFileKind.ZIP)), true),
        StorageRulesPlan(listOf(0), listOf(RecoveryCandidateRef(0, RecoveryFileKind.ZIP)), false),
        StorageRulesPlan(
          emptyList(),
          List(2) { RecoveryCandidateRef(0, RecoveryFileKind.ZIP) },
          false,
        ),
      )) assertFailsWith<IllegalArgumentException> { planStorageRules(request, success(plan)) }
  }

  @Test
  fun nativeFailuresWrongFramesAndSizeErrorsNeverRetry() {
    val request = StorageRulesRequest(operation = StorageOperation.NAMES)
    for (envelope in
      listOf(
        ApiEnvelope(2, "storageRules", StorageRulesPlan(emptyList(), emptyList(), false)),
        ApiEnvelope(1, "other", StorageRulesPlan(emptyList(), emptyList(), false)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planStorageRules(request, respond(rustCbor.encodeToByteArray(envelope)))
      }
    }
    assertFailsWith<RustFailureException> {
      planStorageRules(
        request,
        respond(
          rustCbor.encodeToByteArray(
            ApiEnvelope<StorageRulesPlan>(
              1,
              "storageRules",
              error = RustError(RustErrorKind.ERROR, "fixture"),
            )
          )
        ),
      )
    }
    assertFailsWith<IllegalArgumentException> { planStorageRules(request) { _, _, _ -> 0 } }
    assertFailsWith<IllegalArgumentException> {
      planStorageRules(request) { _, _, out -> out.capacity() + 1 }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      planStorageRules(request) { _, _, _ -> throw UnsatisfiedLinkError("fixture") }
    }
    var calls = 0
    assertFailsWith<IllegalStateException> {
      planStorageRules(request) { _, _, _ ->
        calls++
        error("fixture")
      }
    }
    assertEquals(1, calls)
  }

  @Test
  fun responseCapacityDependsOnOutputSlotsNotLargeIdentities() {
    val request =
      StorageRulesRequest(
        operation = StorageOperation.CANDIDATES,
        maxItems = 1,
        candidates =
          listOf(
            StorageCandidateInput(0, RecoveryFileKind.ZIP, "a".repeat(10_000).toUtf16LeBytes())
          ),
      )
    planStorageRules(request) { _, _, out ->
      assertEquals(576, out.capacity())
      val bytes =
        rustCbor.encodeToByteArray(
          ApiEnvelope(
            1,
            "storageRules",
            StorageRulesPlan(
              emptyList(),
              listOf(RecoveryCandidateRef(0, RecoveryFileKind.ZIP)),
              false,
            ),
          )
        )
      out.put(bytes)
      bytes.size
    }
  }

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/storage_$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun success(plan: StorageRulesPlan) =
    respond(rustCbor.encodeToByteArray(ApiEnvelope(1, "storageRules", plan)))

  private fun respond(bytes: ByteArray): (ByteBuffer, Int, ByteBuffer) -> Int = { _, _, out ->
    out.put(bytes)
    bytes.size
  }
}
