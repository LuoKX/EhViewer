package com.hippo.ehviewer.download

import com.ehviewer.core.database.model.DownloadLabel
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class DownloadLabelImportTest {
  @Test
  fun appendsAfterExistingPositionsInsteadOfImportedCount() {
    val current = (0..4).map { DownloadLabel("old-$it", it, it.toLong()) }
    val source = DownloadLabel("new", 42, 99)
    assertEquals(listOf(DownloadLabel("new", 5)), planDownloadLabelImport(current, listOf(source)))
    assertEquals(DownloadLabel("new", 42, 99), source)
    assertEquals((0..4).toList(), current.map { it.position })
  }

  @Test
  fun skipsCurrentAndBatchDuplicatesWithoutPositionGaps() {
    val result =
      planDownloadLabelImport(
        listOf(DownloadLabel("existing", 0)),
        listOf(
          DownloadLabel("existing", 9),
          DownloadLabel("first", 9),
          DownloadLabel("first", 8),
          DownloadLabel("second", 7),
        ),
      )
    assertEquals(listOf(DownloadLabel("first", 1), DownloadLabel("second", 2)), result)
  }

  @Test
  fun preservesSparsePositionsAndStartsEmptyListsAtZero() {
    assertEquals(
      listOf(DownloadLabel("new", 10)),
      planDownloadLabelImport(
        listOf(DownloadLabel("a", 3), DownloadLabel("b", 9)),
        listOf(DownloadLabel("new", 0)),
      ),
    )
    assertEquals(
      listOf(DownloadLabel("new", 0)),
      planDownloadLabelImport(emptyList(), listOf(DownloadLabel("new", 8))),
    )
    assertEquals(emptyList(), planDownloadLabelImport(emptyList(), emptyList()))
  }

  @Test
  fun checksAllPositionsBeforeMutatingAnyRows() {
    val source = listOf(DownloadLabel("a", 1, 1), DownloadLabel("b", 2, 2))
    assertFailsWith<IllegalArgumentException> {
      planDownloadLabelImport(listOf(DownloadLabel("old", Int.MAX_VALUE - 1)), source)
    }
    assertEquals(listOf(DownloadLabel("a", 1, 1), DownloadLabel("b", 2, 2)), source)
    assertEquals(
      listOf(DownloadLabel("a", Int.MAX_VALUE)),
      planDownloadLabelImport(
        listOf(DownloadLabel("old", Int.MAX_VALUE - 1)),
        source.take(1),
      ),
    )
    assertEquals(
      emptyList(),
      planDownloadLabelImport(
        listOf(DownloadLabel("old", Int.MAX_VALUE)),
        listOf(DownloadLabel("old", 0)),
      ),
    )
  }
}
