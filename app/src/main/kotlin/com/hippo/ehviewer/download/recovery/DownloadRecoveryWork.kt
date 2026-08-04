package com.hippo.ehviewer.download.recovery

import android.content.Context
import android.net.Uri
import android.os.SystemClock
import androidx.work.CoroutineWorker
import androidx.work.Data
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.Settings
import java.util.UUID
import kotlinx.coroutines.CancellationException

internal class DownloadRecoveryWork(
  appContext: Context,
  workerParameters: WorkerParameters,
) : CoroutineWorker(appContext, workerParameters) {
  override suspend fun doWork(): Result {
    if (!inputData.getBoolean(KEY_RUST_BACKEND, false)) return Result.failure()
    val rootUri = inputData.getString(KEY_ROOT_URI)?.let(Uri::parse) ?: return Result.failure()
    val runId = inputData.getString(KEY_RUN_ID) ?: return Result.failure()
    val limits =
      RecoveryScanLimits(
        maxDepth = inputData.getInt(KEY_MAX_DEPTH, DEFAULT_MAX_DEPTH),
        maxNodes = inputData.getInt(KEY_MAX_NODES, DEFAULT_MAX_NODES),
        maxCandidates = inputData.getInt(KEY_MAX_CANDIDATES, DEFAULT_MAX_CANDIDATES),
        maxArchiveEntries =
          inputData.getInt(KEY_MAX_ARCHIVE_ENTRIES, DEFAULT_MAX_ARCHIVE_ENTRIES),
        maxDurationMillis =
          inputData.getLong(KEY_MAX_DURATION_MILLIS, DEFAULT_MAX_DURATION_MILLIS),
      )
    var latest = RecoveryScanProgress()
    var lastPublishedAt = 0L
    setForeground(DownloadRecoveryNotifications.foregroundInfo(applicationContext, id, latest))
    return try {
      val result =
        DownloadRecoveryCoordinator(applicationContext).scan(rootUri, runId, limits) { progress ->
          latest = progress
          val now = SystemClock.elapsedRealtime()
          if (now - lastPublishedAt >= PROGRESS_INTERVAL_MILLIS) {
            lastPublishedAt = now
            publishProgress(progress)
          }
        }
      publishProgress(result.progress)
      Result.success(result.progress.toData(result.limitExceeded))
    } catch (cancelled: CancellationException) {
      throw cancelled
    } catch (failure: Throwable) {
      val failureTypes =
        generateSequence(failure) { it.cause }
          .take(MAX_DIAGNOSTIC_CAUSES)
          .joinToString("<-") { it.javaClass.name }
      val failureFrames =
        failure.stackTrace
          .take(MAX_DIAGNOSTIC_FRAMES)
          .joinToString(";") { "${it.className}.${it.methodName}:${it.lineNumber}" }
      val safeBoundaryMessage =
        failure
          .takeIf {
            it.stackTrace.firstOrNull()?.methodName ==
              DOWNLOAD_RECOVERY_CONFLICT_NATIVE_METHOD
          }
          ?.message
          ?.take(MAX_DIAGNOSTIC_MESSAGE_CHARS)
          .orEmpty()
      val failureDiagnostic =
        "scan_failed types=$failureTypes message=$safeBoundaryMessage stack=$failureFrames"
      val diagnostic =
        IllegalStateException(failureDiagnostic)
      diagnostic.stackTrace = failure.stackTrace
      logcat(DIAGNOSTIC_TAG, diagnostic)
      Result.failure(
        latest.toData(
          limitExceeded = false,
          failed = true,
          failureDiagnostic = failureDiagnostic,
        )
      )
    }
  }

  private suspend fun publishProgress(progress: RecoveryScanProgress) {
    setProgress(progress.toData(limitExceeded = false))
    setForeground(
      DownloadRecoveryNotifications.foregroundInfo(applicationContext, id, progress)
    )
  }

  private fun RecoveryScanProgress.toData(
    limitExceeded: Boolean,
    failed: Boolean = false,
    failureDiagnostic: String = "",
  ): Data =
    workDataOf(
      KEY_DISCOVERED to discovered,
      KEY_CANDIDATES to candidates,
      KEY_PROCESSED to processed,
      KEY_RECOVERED to recovered,
      KEY_PENDING to pending,
      KEY_LIMIT_EXCEEDED to limitExceeded,
      KEY_FAILED to failed,
      KEY_FAILURE_DIAGNOSTIC to failureDiagnostic,
    )

  companion object {
    const val UNIQUE_WORK_NAME = "download-directory-recovery"
    const val KEY_DISCOVERED = "discovered"
    const val KEY_CANDIDATES = "candidates"
    const val KEY_PROCESSED = "processed"
    const val KEY_RECOVERED = "recovered"
    const val KEY_PENDING = "pending"
    const val KEY_LIMIT_EXCEEDED = "limit_exceeded"
    const val KEY_FAILED = "failed"
    const val KEY_FAILURE_DIAGNOSTIC = "failure_diagnostic"

    private const val KEY_ROOT_URI = "root_uri"
    private const val KEY_RUN_ID = "run_id"
    private const val KEY_RUST_BACKEND = "rust_backend"
    private const val KEY_MAX_DEPTH = "max_depth"
    private const val KEY_MAX_NODES = "max_nodes"
    private const val KEY_MAX_CANDIDATES = "max_candidates"
    private const val KEY_MAX_ARCHIVE_ENTRIES = "max_archive_entries"
    private const val KEY_MAX_DURATION_MILLIS = "max_duration_millis"
    private const val DEFAULT_MAX_DEPTH = 32
    private const val DEFAULT_MAX_NODES = 100_000
    private const val DEFAULT_MAX_CANDIDATES = 20_000
    private const val DEFAULT_MAX_ARCHIVE_ENTRIES = 100_000
    private const val DEFAULT_MAX_DURATION_MILLIS = 2 * 60 * 60 * 1_000L
    private const val PROGRESS_INTERVAL_MILLIS = 250L
    private const val DIAGNOSTIC_TAG = "DownloadRecovery"
    private const val MAX_DIAGNOSTIC_CAUSES = 8
    private const val MAX_DIAGNOSTIC_FRAMES = 12
    private const val MAX_DIAGNOSTIC_MESSAGE_CHARS = 512
    private const val DOWNLOAD_RECOVERY_CONFLICT_NATIVE_METHOD =
      "nativePlanDownloadRecoveryConflictsV1"

    fun enqueue(context: Context, rootUri: Uri): UUID {
      require(Settings.useRustCore.value) { "Download recovery requires the Rust core" }
      val runId = UUID.randomUUID().toString()
      val request =
        OneTimeWorkRequestBuilder<DownloadRecoveryWork>()
          .setInputData(
            workDataOf(
              KEY_ROOT_URI to rootUri.toString(),
              KEY_RUN_ID to runId,
              KEY_RUST_BACKEND to true,
              KEY_MAX_DEPTH to DEFAULT_MAX_DEPTH,
              KEY_MAX_NODES to DEFAULT_MAX_NODES,
              KEY_MAX_CANDIDATES to DEFAULT_MAX_CANDIDATES,
              KEY_MAX_ARCHIVE_ENTRIES to DEFAULT_MAX_ARCHIVE_ENTRIES,
              KEY_MAX_DURATION_MILLIS to DEFAULT_MAX_DURATION_MILLIS,
            )
          )
          .addTag(UNIQUE_WORK_NAME)
          .build()
      WorkManager.getInstance(context)
        .enqueueUniqueWork(UNIQUE_WORK_NAME, ExistingWorkPolicy.KEEP, request)
      return request.id
    }

    fun cancel(context: Context) {
      WorkManager.getInstance(context).cancelUniqueWork(UNIQUE_WORK_NAME)
    }
  }
}
