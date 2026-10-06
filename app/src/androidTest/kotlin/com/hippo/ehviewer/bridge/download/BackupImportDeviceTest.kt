package com.hippo.ehviewer.bridge.download

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real JNI contract; connected device execution remains separate acceptance evidence. */
@RunWith(AndroidJUnit4::class)
class BackupImportDeviceTest {
  @Test
  fun combinedPlanUsesFirstSourcesNormalizesStatesAndAppendsLabels() {
    val request =
      BackupImportRequest(
        1,
        listOf(7),
        listOf(
          ImportDownload(7, 1),
          ImportDownload(9, 2),
          ImportDownload(9, 4),
          ImportDownload(-1, Int.MAX_VALUE),
        ),
        listOf(importLabel("old", 9)),
        listOf(
          importLabel("old", 0),
          importLabel("\uD800", 99),
          importLabel("\uD800", 0),
          importLabel("new", -1),
        ),
      )
    assertEquals(
      BackupImportPlan(
        listOf(ImportDownloadAddition(1, 9, 0), ImportDownloadAddition(3, -1, Int.MAX_VALUE)),
        listOf(ImportLabelAddition(1, 10), ImportLabelAddition(3, 11)),
      ),
      planBackupImport(request),
    )
  }

  @Test
  fun overflowReturnsNoPartialPlanAndExtremeEmptyImportsSucceed() {
    val request =
      BackupImportRequest(
        1,
        emptyList(),
        listOf(ImportDownload(1, 1)),
        listOf(importLabel("old", Int.MAX_VALUE)),
        listOf(importLabel("new", 0)),
      )
    assertThrows(IllegalArgumentException::class.java) { planBackupImport(request) }
    assertEquals(
      BackupImportPlan(listOf(ImportDownloadAddition(0, 1, 0)), emptyList()),
      planBackupImport(request.copy(labels = emptyList())),
    )
  }
}
