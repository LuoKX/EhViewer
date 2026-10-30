package com.hippo.ehviewer.download.recovery

import androidx.room.Room
import androidx.room.execSQL
import androidx.room.useWriterConnection
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ehviewer.core.database.EhDatabase
import com.ehviewer.core.database.dao.DownloadRecoveryDao
import com.ehviewer.core.database.dao.RecoveryMergePolicy
import com.ehviewer.core.database.model.*
import com.hippo.ehviewer.bridge.download.RustRecoveryMergePolicy
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Actual generated Room transaction, isolated in-memory database and real JNI policy. */
@RunWith(AndroidJUnit4::class)
class RecoveryPersistenceDeviceTest {
  private fun gallery(title: String, token: String = "fixture", pages: Int = 2) =
    GalleryEntity(1, token, title, null, null, 0, null, null, 0f, null, pages, null, -2)

  private suspend fun database(block: suspend (EhDatabase, DownloadRecoveryDao) -> Unit) {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val db =
      Room.inMemoryDatabaseBuilder<EhDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
    try {
      val dao = db.downloadRecoveryDao()
      dao.upsertGallery(gallery(""))
      dao.upsertDownload(DownloadEntity(gid = 1, state = DownloadInfo.STATE_FAILED, time = 10))
      dao.upsertDirname(DownloadDirname(1, "1"))
      dao.insertArtists(listOf(DownloadArtist(1, " old ")))
      dao.upsert(
        DownloadRecoveryCandidate(
          candidateId = 1,
          sourceKey = "source",
          rootKey = "root",
          sourceLocator = "fixture",
          relativePath = "1",
          displayName = "fixture",
          sourceKind = DownloadRecoverySourceKind.DIRECTORY,
          gid = 1,
          token = "fixture",
          declaredPages = 2,
          localPageCount = 2,
          status = DownloadRecoveryStatus.READY,
          fingerprint = "fingerprint",
          lastSeenRunId = "run",
          createdAt = 10,
          updatedAt = 20,
        )
      )
      block(db, dao)
    } finally {
      db.close()
    }
  }

  private suspend fun promote(
    dao: DownloadRecoveryDao,
    policy: RecoveryMergePolicy = RustRecoveryMergePolicy,
  ) =
    dao.promoteVerified(1, "source", gallery("incoming"), "1", listOf("old", " new "), 100, policy)

  private suspend fun snapshot(dao: DownloadRecoveryDao): List<Any?> =
    listOf(
      dao.load(1),
      dao.loadGallery(1)?.title,
      dao.loadGallery(1)?.token,
      dao.loadGallery(1)?.pages,
      dao.loadDownload(1),
      dao.loadDirname(1),
      dao.loadArtists(1),
    )

  @Test
  fun successfulPromotionUsesFormalMergeAndDeletesOnlyItsCandidate() = runBlocking {
    database { _, dao ->
      val row = promote(dao)
      assertEquals(DownloadInfo.STATE_FINISH, row.state)
      assertEquals("incoming", dao.loadGallery(1)?.title)
      assertEquals(10L, dao.loadDownload(1)?.time)
      assertEquals(setOf("old", "new"), dao.loadArtists(1).map { it.artist }.toSet())
      assertNull(dao.load(1))
    }
  }

  @Test
  fun failuresAtEveryWriteAndFinalReloadRollbackAllEarlierWrites() = runBlocking {
    val faults =
      listOf(
        "BEFORE INSERT ON GALLERIES BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "BEFORE INSERT ON DOWNLOADS BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "BEFORE INSERT ON DOWNLOAD_DIRNAME BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "BEFORE DELETE ON DOWNLOAD_ARTISTS BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "BEFORE INSERT ON DOWNLOAD_ARTISTS BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "BEFORE DELETE ON DOWNLOAD_RECOVERY_CANDIDATES BEGIN SELECT RAISE(ABORT, 'fixture'); END",
        "AFTER DELETE ON DOWNLOAD_RECOVERY_CANDIDATES BEGIN DELETE FROM DOWNLOADS WHERE GID = 1; END",
      )
    for (fault in faults) database { db, dao ->
      val before = snapshot(dao)
      db.useWriterConnection { it.execSQL("CREATE TRIGGER fail_promotion $fault") }
      assertNotNull(runCatching { promote(dao) }.exceptionOrNull())
      assertEquals(before, snapshot(dao))
    }
  }

  @Test
  fun aPathOwnedByAnotherGalleryIsRejectedBeforeAnyMerge() = runBlocking {
    database { db, dao ->
      db.useWriterConnection { it.execSQL("DELETE FROM DOWNLOAD_DIRNAME WHERE GID = 1") }
      dao.upsertDirname(DownloadDirname(2, "1"))
      val before = snapshot(dao)
      val error = runCatching { promote(dao) }.exceptionOrNull()
      assertEquals("The recovery source is already mapped to a different gallery", error?.message)
      assertEquals(before, snapshot(dao))
      assertEquals(DownloadDirname(2, "1"), dao.loadDirname(2))
    }
  }

  @Test
  fun policyFailureAndCancellationAfterInitialWritesRollback() = runBlocking {
    for (failure in
      listOf(IllegalArgumentException("transport"), CancellationException("cancel"))) database {
      _,
      dao ->
      val before = snapshot(dao)
      val policy =
        object : RecoveryMergePolicy by RustRecoveryMergePolicy {
          override fun mergeArtists(existing: List<String>, incoming: List<String>): List<String> =
            throw failure
        }
      assertSame(failure, runCatching { promote(dao, policy) }.exceptionOrNull())
      assertEquals(before, snapshot(dao))
    }
  }

  @Test
  fun staleIdentityCompletenessAndPathChecksRunBeforeTheMergePolicy() = runBlocking {
    val changes: List<suspend (DownloadRecoveryDao) -> Unit> =
      listOf(
        { it.deleteById(1) },
        { it.upsert(checkNotNull(it.load(1)).copy(sourceKey = "changed")) },
        {
          it.upsert(checkNotNull(it.load(1)).copy(status = DownloadRecoveryStatus.SOURCE_MISSING))
        },
        { it.upsert(checkNotNull(it.load(1)).copy(gid = 2)) },
        { it.upsert(checkNotNull(it.load(1)).copy(token = null)) },
        { it.upsert(checkNotNull(it.load(1)).copy(declaredPages = null)) },
        { it.upsert(checkNotNull(it.load(1)).copy(declaredPages = 0)) },
        { it.upsert(checkNotNull(it.load(1)).copy(localPageCount = 1)) },
        { it.upsert(checkNotNull(it.load(1)).copy(relativePath = "changed")) },
        { it.upsertDirname(DownloadDirname(1, "other")) },
        {
          it.upsertDownload(checkNotNull(it.loadDownload(1)).copy(state = DownloadInfo.STATE_WAIT))
        },
        {
          it.upsertDownload(
            checkNotNull(it.loadDownload(1)).copy(state = DownloadInfo.STATE_DOWNLOAD)
          )
        },
        { it.upsertGallery(gallery("", token = "other")) },
        { it.upsertGallery(gallery("", pages = 3)) },
      )
    for (change in changes) database { _, dao ->
      change(dao)
      val before = snapshot(dao)
      val policy =
        object : RecoveryMergePolicy {
          override fun mergeGallery(
            existing: GalleryEntity?,
            incoming: GalleryEntity,
          ): GalleryEntity = error("policy must not run")

          override fun mergeArtists(existing: List<String>, incoming: List<String>): List<String> =
            error("policy must not run")
        }
      val failure = runCatching { promote(dao, policy) }.exceptionOrNull()
      assertNotNull(failure)
      assertNotEquals("policy must not run", failure?.message)
      assertEquals(before, snapshot(dao))
    }
  }
}
