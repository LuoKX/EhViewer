package com.hippo.ehviewer.download.recovery

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.pm.ServiceInfo
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.work.ForegroundInfo
import androidx.work.WorkManager
import com.ehviewer.core.i18n.R
import java.util.UUID

internal object DownloadRecoveryNotifications {
  private const val CHANNEL_ID = "download_recovery"
  private const val NOTIFICATION_ID = 0x524543

  fun foregroundInfo(
    context: Context,
    workId: UUID,
    progress: RecoveryScanProgress,
  ): ForegroundInfo {
    ensureChannel(context)
    val content =
      context.getString(
        R.string.download_recovery_notification_progress,
        progress.discovered,
        progress.candidates,
        progress.processed,
        progress.recovered,
        progress.pending,
      )
    val notification =
      NotificationCompat.Builder(context, CHANNEL_ID)
        .setSmallIcon(android.R.drawable.stat_sys_download)
        .setContentTitle(context.getString(R.string.download_recovery_notification_title))
        .setContentText(content)
        .setStyle(NotificationCompat.BigTextStyle().bigText(content))
        .setCategory(NotificationCompat.CATEGORY_PROGRESS)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setProgress(
          progress.candidates.coerceAtLeast(0),
          progress.processed.coerceAtLeast(0),
          progress.candidates <= 0,
        )
        .addAction(
          0,
          context.getString(R.string.download_recovery_cancel),
          WorkManager.getInstance(context).createCancelPendingIntent(workId),
        )
        .build()
    return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      ForegroundInfo(
        NOTIFICATION_ID,
        notification,
        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
      )
    } else {
      ForegroundInfo(NOTIFICATION_ID, notification)
    }
  }

  private fun ensureChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = context.getSystemService(Service.NOTIFICATION_SERVICE) as NotificationManager
    manager.createNotificationChannel(
      NotificationChannel(
        CHANNEL_ID,
        context.getString(R.string.download_recovery_notification_title),
        NotificationManager.IMPORTANCE_LOW,
      )
    )
  }
}
