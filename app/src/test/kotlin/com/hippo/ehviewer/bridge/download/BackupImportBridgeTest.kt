package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class BackupImportBridgeTest {
  @Test
  fun exactSharedGoldensRoundTripAndDelegateOnce() {
    val input = golden("request")
    val output = golden("response")
    val request = rustCbor.decodeFromByteArray<BackupImportRequest>(input)
    assertContentEquals(input, rustCbor.encodeToByteArray(request))
    val expected = rustCbor.decodeFromByteArray<ApiEnvelope<BackupImportPlan>>(output)
    // Success wire DTO emits explicit CBOR null; production envelope is decode-only here.
    assertContentEquals(
      output,
      rustCbor.encodeToByteArray(
        GoldenSuccessEnvelope(expected.version, expected.module, expected.payload!!, null)
      ),
    )
    assertEquals(
      BackupImportPlan(
        listOf(
          ImportDownloadAddition(1, 2, 0),
          ImportDownloadAddition(3, -9, 0),
          ImportDownloadAddition(4, 3, Int.MIN_VALUE),
        ),
        listOf(ImportLabelAddition(1, 10), ImportLabelAddition(3, 11)),
      ),
      expected.payload,
    )
    var calls = 0
    val actual =
      planBackupImport(request) { buffer, size, response ->
        calls++
        assertTrue(buffer.isDirect && response.isDirect)
        assertContentEquals(input, ByteArray(size).also { buffer.duplicate().get(it) })
        response.put(output)
        output.size
      }
    assertEquals(expected.payload, actual)
    assertEquals(1, calls)
  }

  @Test
  fun invalidRequestsRejectBeforeNativeCall() {
    val base = emptyRequest()
    for (request in
      listOf(
        base.copy(version = 2),
        base.copy(existingGids = List(100_001) { 0L }),
        base.copy(labels = listOf(ImportLabel(byteArrayOf(1), 0))),
        base.copy(currentLabels = listOf(ImportLabel(ByteArray(8194), 0))),
        base.copy(labels = List(513) { ImportLabel(ByteArray(8192), 0) }),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planBackupImport(request) { _, _, _ -> fail("Invalid request entered JNI") }
      }
    }
  }

  @Test
  fun allDownloadIndexIdentityDuplicateOrderAndStateViolationsReject() {
    val request =
      emptyRequest()
        .copy(
          existingGids = listOf(7),
          downloads =
            listOf(
              ImportDownload(7, 1),
              ImportDownload(2, 1),
              ImportDownload(2, 4),
              ImportDownload(-9, 2),
              ImportDownload(3, Int.MIN_VALUE),
            ),
        )
    val a = ImportDownloadAddition(1, 2, 0)
    val b = ImportDownloadAddition(3, -9, 0)
    for (rows in
      listOf(
        listOf(a.copy(sourceIndex = -1)),
        listOf(a.copy(sourceIndex = 5)),
        listOf(a.copy(gid = 99)),
        listOf(a.copy(state = 1)),
        listOf(ImportDownloadAddition(0, 7, 0)),
        listOf(a.copy(sourceIndex = 2, state = 4)),
        listOf(a, a),
        listOf(b, a),
        listOf(ImportDownloadAddition(4, 3, 0)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planBackupImport(request, success(BackupImportPlan(rows, emptyList())))
      }
    }
    assertEquals(
      listOf(a, b),
      planBackupImport(request, success(BackupImportPlan(listOf(a, b), emptyList()))).downloads,
    )
  }

  @Test
  fun allLabelIndicesNamesOrderPositionAndOverflowViolationsReject() {
    val request =
      emptyRequest()
        .copy(
          currentLabels = listOf(importLabel("old", 9)),
          labels =
            listOf(
              importLabel("old", 0),
              importLabel("new", 0),
              importLabel("new", 0),
              importLabel("next", 0),
            ),
        )
    val a = ImportLabelAddition(1, 10)
    val b = ImportLabelAddition(3, 11)
    for (rows in
      listOf(
        listOf(a.copy(sourceIndex = -1)),
        listOf(a.copy(sourceIndex = 4)),
        listOf(a.copy(sourceIndex = 0)),
        listOf(a.copy(sourceIndex = 2)),
        listOf(a.copy(position = -1)),
        listOf(a.copy(position = 9)),
        listOf(a.copy(position = 11)),
        listOf(a, a),
        listOf(b, a),
        listOf(a, b.copy(position = Int.MAX_VALUE)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planBackupImport(request, success(BackupImportPlan(emptyList(), rows)))
      }
    }
    assertEquals(
      listOf(a, b),
      planBackupImport(request, success(BackupImportPlan(emptyList(), listOf(a, b)))).labels,
    )
    assertFailsWith<IllegalArgumentException> {
      planBackupImport(
        request.copy(currentLabels = listOf(importLabel("old", Int.MAX_VALUE))),
        success(BackupImportPlan(emptyList(), listOf(a))),
      )
    }
  }

  @Test
  fun rawJavaNameEqualityHandlesCaseAndLoneSurrogates() {
    val request =
      emptyRequest()
        .copy(
          currentLabels = listOf(importLabel("a", -9)),
          labels =
            listOf(
              importLabel("A", 0),
              importLabel("\uD800", 0),
              importLabel("\uDC00", 0),
              importLabel("", 0),
            ),
        )
    val expected = BackupImportPlan(emptyList(), (0..3).map { ImportLabelAddition(it, it) })
    assertEquals(expected, planBackupImport(request, success(expected)))
  }

  @Test
  fun framesMalformedCborTypedErrorsSizesAndLinkageNeverFallback() {
    val base = emptyRequest()
    val plan = BackupImportPlan(emptyList(), emptyList())
    for (envelope in listOf(ApiEnvelope(2, "backupImport", plan), ApiEnvelope(1, "other", plan))) {
      assertFailsWith<IllegalArgumentException> {
        planBackupImport(base, respond(rustCbor.encodeToByteArray(envelope)))
      }
    }
    assertFails { planBackupImport(base, respond(byteArrayOf(0xff.toByte()))) }
    assertFailsWith<IllegalArgumentException> { planBackupImport(base) { _, _, _ -> 0 } }
    assertFailsWith<IllegalArgumentException> {
      planBackupImport(base) { _, _, out ->
        out.capacity() + 1
      }
    }
    assertFailsWith<RustFailureException> {
      planBackupImport(
        base,
        respond(
          rustCbor.encodeToByteArray(
            ApiEnvelope<BackupImportPlan>(
              1,
              "backupImport",
              error = RustError(RustErrorKind.ERROR, "Invalid backup import input"),
            )
          )
        ),
      )
    }
    val overflow =
      assertFailsWith<IllegalArgumentException> {
        planBackupImport(
          base,
          respond(
            rustCbor.encodeToByteArray(
              ApiEnvelope<BackupImportPlan>(
                1,
                "backupImport",
                error = RustError(RustErrorKind.ERROR, "Download label positions exceed Int range"),
              )
            )
          ),
        )
      }
    assertEquals("Download label positions exceed Int range", overflow.message)
    var calls = 0
    assertFailsWith<UnsatisfiedLinkError> {
      planBackupImport(base) { _, _, _ ->
        calls++
        throw UnsatisfiedLinkError("fixture")
      }
    }
    assertEquals(1, calls)
  }

  @Serializable
  private data class GoldenSuccessEnvelope(
    val version: Int,
    val module: String,
    val payload: BackupImportPlan,
    val error: String?,
  )

  private fun emptyRequest() =
    BackupImportRequest(1, emptyList(), emptyList(), emptyList(), emptyList())

  private fun success(plan: BackupImportPlan) =
    respond(rustCbor.encodeToByteArray(ApiEnvelope(1, "backupImport", plan)))

  private fun respond(bytes: ByteArray): (ByteBuffer, Int, ByteBuffer) -> Int = { _, _, output ->
    output.put(bytes)
    bytes.size
  }

  private fun golden(kind: String): ByteArray =
    File("src/main/rust/crates/domain/test_data/backup_import_$kind.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map {
        it.toInt(16).toByte()
      }
      .toByteArray()
}
