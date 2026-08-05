package com.ehviewer.core.database.dao

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Query
import androidx.room.Transaction
import androidx.room.Upsert
import com.ehviewer.core.database.model.DownloadArtist
import com.ehviewer.core.database.model.DownloadDirname
import com.ehviewer.core.database.model.DownloadEntity
import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.database.model.GalleryEntity
import kotlinx.coroutines.flow.Flow

/** Candidate lifecycle and the only transaction allowed to promote a verified local source. */
@Dao
interface DownloadRecoveryDao {
  @Query("SELECT * FROM DOWNLOAD_RECOVERY_CANDIDATES ORDER BY STATUS, DISPLAY_NAME, SOURCE_KEY")
  fun observeAll(): Flow<List<DownloadRecoveryCandidate>>

  @Query("SELECT * FROM DOWNLOAD_RECOVERY_CANDIDATES ORDER BY STATUS, DISPLAY_NAME, SOURCE_KEY")
  suspend fun list(): List<DownloadRecoveryCandidate>

  @Query("SELECT * FROM DOWNLOAD_RECOVERY_CANDIDATES WHERE CANDIDATE_ID = :candidateId")
  suspend fun load(candidateId: Long): DownloadRecoveryCandidate?

  @Query("SELECT * FROM DOWNLOAD_RECOVERY_CANDIDATES WHERE SOURCE_KEY = :sourceKey")
  suspend fun loadBySourceKey(sourceKey: String): DownloadRecoveryCandidate?

  @Upsert suspend fun upsert(candidate: DownloadRecoveryCandidate)

  /** Reuses the existing local primary key and creation time on every rescan. */
  @Transaction
  suspend fun upsertBySourceKey(candidate: DownloadRecoveryCandidate) {
    val existing = loadBySourceKey(candidate.sourceKey)
    upsert(
      candidate.copy(
        candidateId = existing?.candidateId ?: candidate.candidateId,
        createdAt = existing?.createdAt ?: candidate.createdAt,
      )
    )
  }

  @Delete suspend fun delete(candidate: DownloadRecoveryCandidate)

  @Query("DELETE FROM DOWNLOAD_RECOVERY_CANDIDATES WHERE CANDIDATE_ID = :candidateId")
  suspend fun deleteById(candidateId: Long)

  @Query(
    """
        UPDATE DOWNLOAD_RECOVERY_CANDIDATES
        SET STATUS = 'SOURCE_MISSING', UPDATED_AT = :updatedAt
        WHERE ROOT_KEY = :rootKey AND LAST_SEEN_RUN_ID != :runId
        """
  )
  suspend fun markUnseenSourcesMissing(rootKey: String, runId: String, updatedAt: Long)

  @Query("SELECT * FROM GALLERIES WHERE GID = :gid")
  suspend fun loadGallery(gid: Long): GalleryEntity?

  @Query("SELECT * FROM DOWNLOADS WHERE GID = :gid")
  suspend fun loadDownload(gid: Long): DownloadEntity?

  @Query("SELECT * FROM DOWNLOAD_DIRNAME WHERE GID = :gid")
  suspend fun loadDirname(gid: Long): DownloadDirname?

  @Query("SELECT * FROM DOWNLOAD_DIRNAME WHERE DIRNAME = :relativePath")
  suspend fun loadDirnameByPath(relativePath: String): DownloadDirname?

  @Query("SELECT * FROM DOWNLOAD_ARTISTS WHERE GID = :gid")
  suspend fun loadArtists(gid: Long): List<DownloadArtist>

  @Upsert suspend fun upsertGallery(gallery: GalleryEntity)

  @Upsert suspend fun upsertDownload(download: DownloadEntity)

  @Upsert suspend fun upsertDirname(dirname: DownloadDirname)

  @Query("DELETE FROM DOWNLOAD_ARTISTS WHERE GID = :gid") suspend fun deleteArtists(gid: Long)

  @androidx.room.Insert(onConflict = androidx.room.OnConflictStrategy.IGNORE)
  suspend fun insertArtists(artists: List<DownloadArtist>)

  @Transaction
  @Query(
    """
        SELECT * FROM DOWNLOADS
        LEFT JOIN DOWNLOAD_DIRNAME USING(GID)
        WHERE DOWNLOADS.GID = :gid
        """
  )
  suspend fun loadJoinedDownload(gid: Long): DownloadInfo?

  /**
   * Atomically promotes exactly one candidate after Rust has proven identity and completeness.
   *
   * Existing identity/path conflicts are rechecked inside this transaction. Rich formal metadata
   * wins over sparse recovery metadata, while an active queue row is never rewritten as finished.
   */
  @Transaction
  suspend fun promoteVerified(
    candidateId: Long,
    expectedSourceKey: String,
    incomingGallery: GalleryEntity,
    relativePath: String,
    incomingArtists: List<String>,
    promotedAt: Long,
  ): DownloadInfo {
    val candidate = requireNotNull(load(candidateId)) { "Recovery candidate no longer exists" }
    require(candidate.sourceKey == expectedSourceKey) {
      "Recovery candidate changed before promotion"
    }
    require(candidate.status == DownloadRecoveryStatus.READY) { "Recovery candidate is not ready" }
    require(candidate.gid == incomingGallery.gid) {
      "Recovery candidate identity changed before promotion"
    }
    require(candidate.token != null && candidate.token == incomingGallery.token) {
      "Recovery candidate token changed before promotion"
    }
    require(candidate.declaredPages != null && candidate.declaredPages > 0) {
      "Recovery candidate has no trusted page count"
    }
    require(candidate.localPageCount == candidate.declaredPages) {
      "Recovery candidate is not locally complete"
    }
    require(relativePath == candidate.relativePath) {
      "Recovery source path changed before promotion"
    }

    val gid = incomingGallery.gid
    val existingPath = loadDirname(gid)
    require(existingPath == null || existingPath.dirname == relativePath) {
      "A valid source mapping already exists for this gallery"
    }
    val pathOwner = loadDirnameByPath(relativePath)
    require(pathOwner == null || pathOwner.gid == gid) {
      "The recovery source is already mapped to a different gallery"
    }

    val existingDownload = loadDownload(gid)
    require(
      existingDownload?.state != DownloadInfo.STATE_WAIT &&
        existingDownload?.state != DownloadInfo.STATE_DOWNLOAD
    ) {
      "An active download cannot be replaced by recovery"
    }

    val existingGallery = loadGallery(gid)
    require(existingGallery == null || existingGallery.token == incomingGallery.token) {
      "The formal gallery identity conflicts with the recovery candidate"
    }
    require(
      existingGallery == null ||
        existingGallery.pages <= 0 ||
        incomingGallery.pages <= 0 ||
        existingGallery.pages == incomingGallery.pages
    ) {
      "The formal gallery page count conflicts with the recovery candidate"
    }

    upsertGallery(mergeGallery(existingGallery, incomingGallery))
    upsertDownload(
      existingDownload?.apply { state = DownloadInfo.STATE_FINISH }
        ?: DownloadEntity(
          gid = gid,
          state = DownloadInfo.STATE_FINISH,
          legacy = 0,
          time = promotedAt,
        )
    )
    upsertDirname(DownloadDirname(gid, relativePath))

    val mergedArtists =
      (loadArtists(gid).map(DownloadArtist::artist) + incomingArtists)
        .map(String::trim)
        .filter(String::isNotEmpty)
        .distinct()
    if (mergedArtists.isNotEmpty()) {
      deleteArtists(gid)
      insertArtists(DownloadArtist.from(gid, mergedArtists))
    }

    deleteById(candidateId)
    return requireNotNull(loadJoinedDownload(gid)) { "Promoted download could not be reloaded" }
  }

  private fun mergeGallery(existing: GalleryEntity?, incoming: GalleryEntity): GalleryEntity {
    if (existing == null) return incoming
    return GalleryEntity(
      gid = existing.gid,
      token = existing.token,
      title = existing.title.takeUnless(String?::isNullOrBlank) ?: incoming.title,
      titleJpn = existing.titleJpn.takeUnless(String?::isNullOrBlank) ?: incoming.titleJpn,
      thumbKey = existing.thumbKey.takeUnless(String?::isNullOrBlank) ?: incoming.thumbKey,
      category = existing.category.takeUnless { it == 0 } ?: incoming.category,
      posted = existing.posted.takeUnless(String?::isNullOrBlank) ?: incoming.posted,
      uploader = existing.uploader.takeUnless(String?::isNullOrBlank) ?: incoming.uploader,
      rating = existing.rating.takeUnless { it == 0f } ?: incoming.rating,
      simpleTags = existing.simpleTags?.takeIf(List<String>::isNotEmpty) ?: incoming.simpleTags,
      pages = existing.pages.takeUnless { it <= 0 } ?: incoming.pages,
      simpleLanguage =
        existing.simpleLanguage.takeUnless(String?::isNullOrBlank) ?: incoming.simpleLanguage,
      favoriteSlot = existing.favoriteSlot,
    )
  }
}
