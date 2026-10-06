package com.hippo.ehviewer.download

import com.ehviewer.core.data.model.asEntity
import com.ehviewer.core.database.model.DownloadArtist
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class DownloadBackupImportTest {
  @Test
  fun delegatesExclusionFirstGidAndStateNormalizationOnceWithoutChangingSources() {
    val first =
      info(2, 1).apply {
        legacy = 91
        time = 72
        label = "saved"
        speed = 33
        remaining = 44
        finished = 5
        downloaded = 6
        total = 7
        artistInfoList = listOf(DownloadArtist(2, "artist"))
      }
    val duplicate = info(2, 4)
    val source = listOf(info(7, 2), first, duplicate, info(-9, 2), info(3, Int.MIN_VALUE))
    var calls = 0
    val result =
      planDownloadImport(listOf(7L), source) { request ->
        assertEquals(listOf(7L), request.existingGids)
        assertEquals(listOf(2, 1, 4, 2, Int.MIN_VALUE), request.downloads.map { it.state })
        planBackupImport(request) { _, _, output ->
          calls++
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(
                1,
                "backupImport",
                BackupImportPlan(
                  listOf(
                    ImportDownloadAddition(1, 2, 0),
                    ImportDownloadAddition(3, -9, 0),
                    ImportDownloadAddition(4, 3, Int.MIN_VALUE),
                  ),
                  emptyList(),
                ),
              )
            )
          output.put(bytes)
          bytes.size
        }
      }
    assertEquals(1, calls)
    assertEquals(listOf(2L, -9L, 3L), result.map { it.gid })
    assertEquals(listOf(0, 0, Int.MIN_VALUE), result.map { it.state })
    assertNotSame(first, result[0])
    assertNotSame(first.downloadInfo, result[0].downloadInfo)
    assertEquals(first.downloadInfo.copy(state = 0), result[0].downloadInfo)
    assertEquals(first.dirname, result[0].dirname)
    assertEquals(first.artistInfoList, result[0].artistInfoList)
    assertEquals(listOf(2, 1, 4, 2, Int.MIN_VALUE), source.map { it.state })
  }

  @Test
  fun unknownFinalIndexRejectsWholePlanBeforeWrites() {
    val source = listOf(info(1, 1), info(2, 2))
    var writes = 0
    assertFailsWith<IllegalArgumentException> {
      planDownloadImport(emptyList(), source) { request ->
          planBackupImport(request) { _, _, output ->
            val bytes =
              rustCbor.encodeToByteArray(
                ApiEnvelope(
                  1,
                  "backupImport",
                  BackupImportPlan(
                    listOf(ImportDownloadAddition(0, 1, 0), ImportDownloadAddition(9, 2, 0)),
                    emptyList(),
                  ),
                )
              )
            output.put(bytes)
            bytes.size
          }
        }
        .forEach { writes++ }
    }
    assertEquals(0, writes)
    assertEquals(listOf(1, 2), source.map { it.state })
  }

  @Test
  fun sourceStateGidIdentityAndCurrentSnapshotChangesReject() {
    for (mutation in 0..3) {
      val source = mutableListOf(info(1, 1))
      val current = mutableListOf(7L)
      assertFailsWith<IllegalArgumentException> {
        planDownloadImport(current, source) {
          when (mutation) {
            0 -> source[0].state = 4
            1 -> source[0].galleryInfo.gid = 2
            2 -> source[0] = info(1, 1)
            3 -> current[0] = 8L
          }
          BackupImportPlan(listOf(ImportDownloadAddition(0, 1, 0)), emptyList())
        }
      }
    }
  }

  @Test
  fun nativeFailureHasNoFallbackAndNoInputMutation() {
    val source = info(1, 1)
    var calls = 0
    assertFailsWith<UnsatisfiedLinkError> {
      planDownloadImport(emptyList(), listOf(source)) {
        calls++
        throw UnsatisfiedLinkError("fixture")
      }
    }
    assertEquals(1, calls)
    assertEquals(1, source.state)
  }

  private fun info(
    gid: Long,
    state: Int,
  ) =
    DownloadInfo(BaseGalleryInfo().apply { this.gid = gid }.asEntity(), "directory").apply {
      this.state = state
    }
}
