package com.hippo.ehviewer.download

import com.ehviewer.core.database.model.DownloadLabel
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class DownloadLabelImportTest {
  @Test
  fun appendsAfterExistingPositionsInsteadOfImportedCountThroughRust() {
    val current = (0..4).map { DownloadLabel("old-$it", it, it.toLong()) }
    val source = DownloadLabel("new", 42, 99)
    assertEquals(
      listOf(DownloadLabel("new", 5)),
      delegated(current, listOf(source), listOf(ImportLabelAddition(0, 5))),
    )
    assertEquals(DownloadLabel("new", 42, 99), source)
    assertEquals((0..4).toList(), current.map { it.position })
  }

  @Test
  fun skipsCurrentAndBatchDuplicatesWithoutPositionGapsThroughRust() {
    val result =
      delegated(
        listOf(DownloadLabel("existing", 0)),
        listOf(
          DownloadLabel("existing", 9),
          DownloadLabel("first", 9),
          DownloadLabel("first", 8),
          DownloadLabel("second", 7),
        ),
        listOf(ImportLabelAddition(1, 1), ImportLabelAddition(3, 2)),
      )
    assertEquals(listOf(DownloadLabel("first", 1), DownloadLabel("second", 2)), result)
  }

  @Test
  fun preservesSparseNegativeAndEmptyPositionsThroughRust() {
    assertEquals(
      listOf(DownloadLabel("new", 10)),
      delegated(
        listOf(DownloadLabel("a", 3), DownloadLabel("b", 9)),
        listOf(DownloadLabel("new", 0)),
        listOf(ImportLabelAddition(0, 10)),
      ),
    )
    assertEquals(
      listOf(DownloadLabel("new", 0)),
      delegated(emptyList(), listOf(DownloadLabel("new", 8)), listOf(ImportLabelAddition(0, 0))),
    )
    assertEquals(
      listOf(DownloadLabel("new", 0)),
      delegated(
        listOf(DownloadLabel("old", Int.MIN_VALUE)),
        listOf(DownloadLabel("new", 8)),
        listOf(ImportLabelAddition(0, 0)),
      ),
    )
    assertEquals(emptyList(), delegated(emptyList(), emptyList(), emptyList()))
    assertEquals(
      emptyList(),
      delegated(listOf(DownloadLabel("old", Int.MAX_VALUE)), emptyList(), emptyList()),
    )
  }

  @Test
  fun overflowRejectsBeforeAnyApplicationAndPreservesInputs() {
    val source = listOf(DownloadLabel("a", 1, 1), DownloadLabel("b", 2, 2))
    var writes = 0
    assertFailsWith<IllegalArgumentException> {
      planDownloadLabelImport(listOf(DownloadLabel("old", Int.MAX_VALUE - 1)), source) { request ->
          planBackupImport(request) { _, _, output ->
            val bytes =
              rustCbor.encodeToByteArray(
                ApiEnvelope<BackupImportPlan>(
                  1,
                  "backupImport",
                  error =
                    RustError(RustErrorKind.ERROR, "Download label positions exceed Int range"),
                )
              )
            output.put(bytes)
            bytes.size
          }
        }
        .forEach { writes++ }
    }
    assertEquals(0, writes)
    assertEquals(listOf(DownloadLabel("a", 1, 1), DownloadLabel("b", 2, 2)), source)
    assertEquals(
      listOf(DownloadLabel("a", Int.MAX_VALUE)),
      delegated(
        listOf(
          DownloadLabel(
            "old",
            Int.MAX_VALUE - 1,
          )
        ),
        source.take(1),
        listOf(ImportLabelAddition(0, Int.MAX_VALUE)),
      ),
    )
    assertEquals(
      emptyList(),
      delegated(
        listOf(DownloadLabel("old", Int.MAX_VALUE)),
        listOf(DownloadLabel("old", 0)),
        emptyList(),
      ),
    )
  }

  @Test
  fun invalidWholePlanNeverCreatesPartialWritesOrModifiesSources() {
    val source = listOf(DownloadLabel("a", 0, 7), DownloadLabel("b", 0, 8))
    var writes = 0
    assertFailsWith<IllegalArgumentException> {
      delegated(
          emptyList(),
          source,
          listOf(ImportLabelAddition(0, 0), ImportLabelAddition(99, 1)),
        )
        .forEach {
          writes++
        }
    }
    assertEquals(0, writes)
    assertEquals(listOf(7L, 8L), source.map { it.id })
  }

  @Test
  fun changedSnapshotAndLinkageFailureNeverApplyOrFallback() {
    val source = DownloadLabel("a", 0, 7)
    assertFailsWith<IllegalArgumentException> {
      planDownloadLabelImport(emptyList(), listOf(source)) { _ ->
        source.label = "changed"
        BackupImportPlan(emptyList(), listOf(ImportLabelAddition(0, 0)))
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      planDownloadLabelImport(emptyList(), listOf(source)) { throw UnsatisfiedLinkError("fixture") }
    }
  }

  private fun delegated(
    current: List<DownloadLabel>,
    source: List<DownloadLabel>,
    rows: List<ImportLabelAddition>,
  ): List<DownloadLabel> {
    var calls = 0
    val result =
      planDownloadLabelImport(current, source) { request ->
        assertEquals(current.map { it.position }, request.currentLabels.map { it.position })
        source.forEachIndexed { index, row ->
          assertContentEquals(
            importLabel(row.label, row.position).labelUtf16,
            request.labels[index].labelUtf16,
          )
        }
        planBackupImport(request) { _, _, output ->
          calls++
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(1, "backupImport", BackupImportPlan(emptyList(), rows))
            )
          output.put(bytes)
          bytes.size
        }
      }
    assertEquals(1, calls)
    result.forEach {
      assertNull(it.id)
      assertTrue(source.none { input -> input === it })
    }
    return result
  }
}
