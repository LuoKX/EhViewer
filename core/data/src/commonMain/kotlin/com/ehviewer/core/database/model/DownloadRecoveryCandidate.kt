package com.ehviewer.core.database.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import androidx.room.TypeConverter
import androidx.room.TypeConverters
import kotlin.time.Clock

/** The Android-owned local source shape represented by a staged recovery row. */
enum class DownloadRecoverySourceKind {
  DIRECTORY,
  CBZ,
  ZIP,
  RAR,
  RAR5,
  SEVEN_ZIP,
  TAR,
  TAR_GZ,
  TAR_XZ,
}

/** Stable, machine-readable reason why a candidate has not been promoted. */
enum class DownloadRecoveryStatus {
  MISSING_IDENTITY,
  CONTENT_INCOMPLETE,
  UNREADABLE,
  UNSUPPORTED,
  IDENTITY_CONFLICT,
  DUPLICATE_SOURCE,
  PATH_CONFLICT,
  QUERY_FAILED,
  SOURCE_MISSING,
  LIMIT_EXCEEDED,
  READY,
}

class DownloadRecoveryConverters {
  @TypeConverter fun sourceKindToString(value: DownloadRecoverySourceKind): String = value.name

  @TypeConverter
  fun stringToSourceKind(value: String): DownloadRecoverySourceKind =
    DownloadRecoverySourceKind.valueOf(value)

  @TypeConverter fun statusToString(value: DownloadRecoveryStatus): String = value.name

  @TypeConverter
  fun stringToStatus(value: String): DownloadRecoveryStatus = DownloadRecoveryStatus.valueOf(value)
}

/**
 * A local recovery candidate kept separate from the formal download tables.
 *
 * [candidateId] is intentionally local and [gid] is nullable: unresolved local sources never borrow
 * a fabricated gallery identity. Secret fields are explicitly redacted from [toString].
 */
@Entity(
  tableName = "DOWNLOAD_RECOVERY_CANDIDATES",
  indices =
    [
      Index(value = ["SOURCE_KEY"], unique = true),
      Index(value = ["ROOT_KEY"]),
      Index(value = ["STATUS"]),
      Index(value = ["GID"]),
    ],
)
@TypeConverters(DownloadRecoveryConverters::class)
data class DownloadRecoveryCandidate(
  @PrimaryKey(autoGenerate = true) @ColumnInfo(name = "CANDIDATE_ID") val candidateId: Long = 0,
  @ColumnInfo(name = "SOURCE_KEY") val sourceKey: String,
  @ColumnInfo(name = "ROOT_KEY") val rootKey: String,
  @ColumnInfo(name = "SOURCE_LOCATOR") val sourceLocator: String,

  /** Path relative to the configured download root; never reconstructed from a display name. */
  @ColumnInfo(name = "RELATIVE_PATH") val relativePath: String,
  @ColumnInfo(name = "DISPLAY_NAME") val displayName: String,
  @ColumnInfo(name = "SOURCE_KIND") val sourceKind: DownloadRecoverySourceKind,
  @ColumnInfo(name = "GID") val gid: Long? = null,
  @ColumnInfo(name = "TOKEN") val token: String? = null,
  @ColumnInfo(name = "PAGE_TOKEN") val pageToken: String? = null,
  @ColumnInfo(name = "PAGE_INDEX") val pageIndex: Int? = null,
  @ColumnInfo(name = "TITLE") val title: String? = null,
  @ColumnInfo(name = "TITLE_JPN") val titleJpn: String? = null,
  @ColumnInfo(name = "DECLARED_PAGES") val declaredPages: Int? = null,
  @ColumnInfo(name = "LOCAL_PAGE_COUNT") val localPageCount: Int = 0,

  /** Stable comma-separated evidence identifiers; never contains source bytes or tokens. */
  @ColumnInfo(name = "EVIDENCE") val evidence: String = "",

  /** Stable comma-separated issue identifiers; never contains source bytes or tokens. */
  @ColumnInfo(name = "ISSUES") val issues: String = "",
  @ColumnInfo(name = "STATUS") val status: DownloadRecoveryStatus,
  @ColumnInfo(name = "FINGERPRINT") val fingerprint: String,
  @ColumnInfo(name = "LAST_SEEN_RUN_ID") val lastSeenRunId: String,
  @ColumnInfo(name = "CREATED_AT") val createdAt: Long = Clock.System.now().toEpochMilliseconds(),
  @ColumnInfo(name = "UPDATED_AT") val updatedAt: Long = createdAt,
) {
  override fun toString(): String =
    "DownloadRecoveryCandidate(" +
      "candidateId=$candidateId, sourceKey=$sourceKey, rootKey=$rootKey, " +
      "sourceLocator=$sourceLocator, relativePath=$relativePath, displayName=$displayName, " +
      "sourceKind=$sourceKind, gid=$gid, token=<redacted>, pageToken=<redacted>, " +
      "pageIndex=$pageIndex, title=$title, titleJpn=$titleJpn, " +
      "declaredPages=$declaredPages, localPageCount=$localPageCount, evidence=$evidence, " +
      "issues=$issues, status=$status, fingerprint=$fingerprint, " +
      "lastSeenRunId=$lastSeenRunId, createdAt=$createdAt, updatedAt=$updatedAt)"
}
