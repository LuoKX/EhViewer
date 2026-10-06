package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.download.RecoveryIssue
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
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.cancel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class ArchiveRecoveryBridgeTest {
  private val request = ArchiveRecoveryRequest(3, 0, 32, 100_000, true)
  private val payload =
    ArchiveRecoveryBatch(
      2,
      2,
      true,
      true,
      emptyList(),
      listOf(
        ArchiveRecoveryEntry("1.jpg", 4, true),
        ArchiveRecoveryEntry("2.jpg", 4, true),
      ),
    )

  @Test
  fun sharedRustGoldensPinRequestAndEveryResponseField() {
    assertContentEquals(golden("request"), rustCbor.encodeToByteArray(request))
    assertEquals(payload, rustCbor.decodeFromByteArray<ArchiveRecoveryBatch>(golden("response")))
    assertContentEquals(golden("response"), rustCbor.encodeToByteArray(payload))
    var calls = 0
    val result =
      inspectArchiveRecovery(request) { buffer, size ->
        calls += 1
        assertTrue(buffer.isDirect)
        assertEquals(0, buffer.position())
        assertEquals(size, buffer.remaining())
        assertContentEquals(golden("request"), ByteArray(size).also { buffer.duplicate().get(it) })
        response(payload)
      }
    assertEquals(payload, result)
    assertEquals(1, calls)
    assertTrue(result.entries.first().toRecoveryEntry().regular)
  }

  @Test
  fun rejectsInvalidRequestsBeforeNativeCalls() {
    for (bad in
      listOf(
        request.copy(version = 2),
        request.copy(startIndex = -1),
        request.copy(maxEntries = 0),
        request.copy(maxEntries = 33),
        request.copy(maximumEntries = 0),
        request.copy(allReadable = false),
      )) {
      assertFailsWith<IllegalArgumentException> {
        inspectArchiveRecovery(bad) { _, _ -> error("Invalid request reached native") }
      }
    }
  }

  @Test
  fun rejectsMalformedBatchesBeforeApplyingFacts() {
    val invalid =
      listOf(
        payload.copy(nextIndex = 1),
        payload.copy(entryCount = -1),
        payload.copy(entryCount = 100_001),
        payload.copy(complete = false),
        payload.copy(allReadable = false),
        payload.copy(entries = emptyList(), nextIndex = 0),
        payload.copy(entries = payload.entries.map { it.copy(name = "") }),
        payload.copy(entries = payload.entries.map { it.copy(size = 0) }),
        payload.copy(entries = payload.entries.map { it.copy(size = 512L * 1024 * 1024 + 1) }),
        payload.copy(entries = payload.entries.map { it.copy(name = "x".repeat(65_537)) }),
        payload.copy(issues = listOf(RecoveryIssue.MISSING_GID)),
        payload.copy(
          issues = listOf(RecoveryIssue.UNSAFE_ARCHIVE_PATH, RecoveryIssue.UNSAFE_ARCHIVE_PATH)
        ),
        payload.copy(issues = listOf(RecoveryIssue.ENCRYPTED_ARCHIVE)),
        payload.copy(issues = listOf(RecoveryIssue.LIMIT_EXCEEDED)),
      )
    for (bad in invalid) {
      assertFailsWith<IllegalArgumentException> {
        inspectArchiveRecovery(request) { _, _ -> response(bad) }
      }
    }
    for (bytes in listOf(byteArrayOf(), byteArrayOf(0), ByteArray(3 * 1024 * 1024 + 1))) {
      assertFailsWith<Exception> { inspectArchiveRecovery(request) { _, _ -> bytes } }
    }
    assertFailsWith<IllegalArgumentException> {
      inspectArchiveRecovery(request) { _, _ -> response(payload, 2) }
    }
    assertFailsWith<IllegalArgumentException> {
      inspectArchiveRecovery(request) { _, _ -> response(payload, module = "gallery") }
    }
  }

  @Test
  fun acceptsStructuredEncryptionLimitAndCumulativeUnreadability() {
    val encrypted =
      ArchiveRecoveryBatch(0, 0, true, false, listOf(RecoveryIssue.ENCRYPTED_ARCHIVE), emptyList())
    assertEquals(encrypted, inspectArchiveRecovery(request) { _, _ -> response(encrypted) })
    val limited =
      ArchiveRecoveryBatch(2, 0, true, false, listOf(RecoveryIssue.LIMIT_EXCEEDED), emptyList())
    assertEquals(
      limited,
      inspectArchiveRecovery(request.copy(maximumEntries = 1)) { _, _ -> response(limited) },
    )
    val last =
      payload.copy(
        entries = payload.entries.takeLast(1),
        allReadable = false,
        complete = false,
      )
    assertEquals(
      last,
      inspectArchiveRecovery(request.copy(startIndex = 1, allReadable = false)) { _, _ ->
        response(last)
      },
    )
    assertFailsWith<IllegalArgumentException> {
      inspectArchiveRecovery(request.copy(startIndex = 1, allReadable = false)) { _, _ ->
        response(last.copy(allReadable = true, complete = true))
      }
    }
  }

  @Test
  fun surfacesDomainAndLinkageFailureWithoutFallback() {
    val bytes =
      rustCbor.encodeToByteArray(
        ApiEnvelope<ArchiveRecoveryBatch>(
          3,
          "archiveRecovery",
          error = RustError(RustErrorKind.ERROR, "batch failed"),
        )
      )
    assertFailsWith<RustFailureException> { inspectArchiveRecovery(request) { _, _ -> bytes } }
    val failure = UnsatisfiedLinkError("batch JNI unavailable")
    assertSame(
      failure,
      assertFailsWith<UnsatisfiedLinkError> {
        inspectArchiveRecovery(request) { _, _ -> throw failure }
      },
    )
  }

  @Test
  fun collectorCarriesRustStateAndNeverRecomputesCompletion() = runBlocking {
    val requests = ArrayList<ArchiveRecoveryRequest>()
    val first =
      payload.copy(
        nextIndex = 1,
        allReadable = false,
        complete = false,
        entries = listOf(payload.entries.first().copy(readable = false)),
      )
    val last =
      payload.copy(
        allReadable = false,
        complete = false,
        entries = payload.entries.takeLast(1),
      )
    val result =
      collectArchiveRecoveryBatches(100_000, { false }) { next ->
        requests.add(next)
        if (next.startIndex == 0) first else last
      }
    assertEquals(listOf(0, 1), requests.map { it.startIndex })
    assertEquals(listOf(true, false), requests.map { it.allReadable })
    assertEquals(2, result.entries.size)
    assertFalse(result.complete)
  }

  @Test
  fun collectorStopsAtDeadlinesBeforeAndBetweenBatches() = runBlocking {
    val expired =
      collectArchiveRecoveryBatches(1, { true }) { error("Expired scan must not inspect") }
    assertEquals(listOf(RecoveryIssue.LIMIT_EXCEEDED), expired.issues)
    var calls = 0
    val partial = payload.copy(nextIndex = 1, complete = false, entries = payload.entries.take(1))
    val result =
      collectArchiveRecoveryBatches(100_000, { calls > 0 }) {
        calls += 1
        partial
      }
    assertEquals(1, calls)
    assertEquals(1, result.entries.size)
    assertFalse(result.complete)
    val finalExpired =
      collectArchiveRecoveryBatches(100_000, { calls > 1 }) {
        calls += 1
        payload
      }
    assertFalse(finalExpired.complete)
  }

  @Test
  fun collectorRejectsChangingIndexesAndStopsOnNativeFailure() = runBlocking {
    val partial = payload.copy(nextIndex = 1, complete = false, entries = payload.entries.take(1))
    assertFailsWith<IllegalStateException> {
      collectArchiveRecoveryBatches(100_000, { false }) { next ->
        if (next.startIndex == 0) partial else payload.copy(entryCount = 3)
      }
    }
    var calls = 0
    assertFailsWith<UnsatisfiedLinkError> {
      collectArchiveRecoveryBatches(100_000, { false }) {
        calls += 1
        throw UnsatisfiedLinkError("missing")
      }
    }
    assertEquals(1, calls)
  }

  @Test
  fun collectorObservesCancellationBeforeAnotherNativeCall() = runBlocking {
    var calls = 0
    val job =
      launch(start = CoroutineStart.UNDISPATCHED) {
        val context = currentCoroutineContext()
        assertFailsWith<CancellationException> {
          collectArchiveRecoveryBatches(100_000, { false }) {
            calls += 1
            context.cancel()
            payload.copy(nextIndex = 1, complete = false, entries = payload.entries.take(1))
          }
        }
      }
    job.join()
    assertTrue(job.isCancelled)
    assertEquals(1, calls)
  }

  private fun response(
    value: ArchiveRecoveryBatch,
    version: Int = 3,
    module: String = "archiveRecovery",
  ) = rustCbor.encodeToByteArray(ApiEnvelope(version, module, payload = value))

  private fun golden(kind: String): ByteArray {
    val relative = "src/main/rust/crates/archive/test_data/recovery_batch_$kind.cbor.hex"
    val file = listOf(File(relative), File("app/$relative")).first { it.isFile }
    val hex = file.readText().trim()
    return ByteArray(hex.length / 2) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
  }
}
