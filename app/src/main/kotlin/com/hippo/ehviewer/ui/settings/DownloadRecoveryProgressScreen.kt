package com.hippo.ehviewer.ui.settings

import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.work.Data
import androidx.work.WorkInfo
import androidx.work.WorkManager
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.i18n.R
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.download.recovery.DownloadRecoveryWork
import com.hippo.ehviewer.ui.Screen
import com.hippo.ehviewer.ui.destinations.DownloadRecoveryCandidatesScreenDestination
import com.ramcosta.composedestinations.annotation.Destination
import com.ramcosta.composedestinations.annotation.RootGraph
import com.ramcosta.composedestinations.navigation.DestinationsNavigator
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.withContext

@Destination<RootGraph>
@Composable
fun AnimatedVisibilityScope.DownloadRecoveryProgressScreen(
  navigator: DestinationsNavigator
) = Screen(navigator) {
  val context = LocalContext.current
  var workInfo by remember { mutableStateOf<WorkInfo?>(null) }
  val candidates by EhDB.downloadRecoveryCandidates.collectAsState(initial = emptyList())
  val missing = candidates.filter { it.status == DownloadRecoveryStatus.MISSING_IDENTITY }
  var completionHandled by rememberSaveable { mutableStateOf(false) }
  var showMissingDialog by rememberSaveable { mutableStateOf(false) }

  LaunchedEffect(Unit) {
    while (isActive) {
      workInfo =
        withContext(Dispatchers.IO) {
          WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork(DownloadRecoveryWork.UNIQUE_WORK_NAME)
            .get(30, TimeUnit.SECONDS)
            .lastOrNull()
        }
      if (workInfo?.state?.isFinished == true) break
      delay(500)
    }
  }
  LaunchedEffect(workInfo?.state, missing.size) {
    if (workInfo?.state?.isFinished == true && missing.isNotEmpty() && !completionHandled) {
      completionHandled = true
      showMissingDialog = true
    }
  }

  val data =
    workInfo?.let { info -> if (info.state.isFinished) info.outputData else info.progress } ?: Data.EMPTY
  val discovered = data.getInt(DownloadRecoveryWork.KEY_DISCOVERED, 0)
  val total = data.getInt(DownloadRecoveryWork.KEY_CANDIDATES, 0)
  val processed = data.getInt(DownloadRecoveryWork.KEY_PROCESSED, 0)
  val recovered = data.getInt(DownloadRecoveryWork.KEY_RECOVERED, 0)
  val pending = data.getInt(DownloadRecoveryWork.KEY_PENDING, candidates.size)
  val running = workInfo?.state == WorkInfo.State.RUNNING || workInfo?.state == WorkInfo.State.ENQUEUED

  Column(
    modifier =
      Modifier.fillMaxSize()
        .verticalScroll(rememberScrollState())
        .padding(24.dp),
    verticalArrangement = Arrangement.spacedBy(16.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
  ) {
    Text(
      text = stringResource(R.string.download_recovery_progress_title),
      style = MaterialTheme.typography.headlineSmall,
    )
    if (running) CircularProgressIndicator()
    Text(
      text =
        stringResource(
          R.string.download_recovery_progress_counters,
          discovered,
          total,
          processed,
          recovered,
          pending,
        ),
      style = MaterialTheme.typography.bodyLarge,
    )
    val stateText =
      when (workInfo?.state) {
        WorkInfo.State.SUCCEEDED -> stringResource(R.string.download_recovery_finished)
        WorkInfo.State.FAILED -> stringResource(R.string.download_recovery_failed)
        WorkInfo.State.CANCELLED -> stringResource(R.string.download_recovery_cancelled)
        WorkInfo.State.BLOCKED,
        WorkInfo.State.ENQUEUED,
        WorkInfo.State.RUNNING,
        null -> stringResource(R.string.download_recovery_running)
      }
    Text(stateText, style = MaterialTheme.typography.bodyMedium)
    if (workInfo?.state == WorkInfo.State.FAILED) {
      data
        .getString(DownloadRecoveryWork.KEY_FAILURE_DIAGNOSTIC)
        ?.takeIf(String::isNotBlank)
        ?.let { diagnostic ->
          Text(diagnostic, style = MaterialTheme.typography.bodySmall)
        }
    }
    Row(
      modifier = Modifier.fillMaxWidth(),
      horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.CenterHorizontally),
    ) {
      if (running) {
        OutlinedButton(onClick = { DownloadRecoveryWork.cancel(context) }) {
          Text(stringResource(R.string.download_recovery_cancel))
        }
      }
      Button(onClick = { navigator.navigate(DownloadRecoveryCandidatesScreenDestination()) }) {
        Text(stringResource(R.string.download_recovery_view_pending))
      }
    }
  }

  if (showMissingDialog) {
    AlertDialog(
      onDismissRequest = {},
      title = { Text(stringResource(R.string.download_recovery_missing_title)) },
      text = {
        Column(
          modifier = Modifier.verticalScroll(rememberScrollState()),
          verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
          Text(stringResource(R.string.download_recovery_missing_message))
          missing.forEach { candidate ->
            Text("• ${candidate.relativePath}", style = MaterialTheme.typography.bodySmall)
          }
        }
      },
      dismissButton = {
        OutlinedButton(
          onClick = {
            showMissingDialog = false
            navigator.popBackStack()
          }
        ) {
          Text(stringResource(R.string.download_recovery_exit))
        }
      },
      confirmButton = {
        Button(
          onClick = {
            showMissingDialog = false
            navigator.navigate(DownloadRecoveryCandidatesScreenDestination(autoLookup = true))
          }
        ) {
          Text(stringResource(R.string.download_recovery_fetch_from_eh))
        }
      },
    )
  }
}
