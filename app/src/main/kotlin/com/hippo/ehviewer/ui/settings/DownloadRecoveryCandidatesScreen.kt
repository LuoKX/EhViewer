package com.hippo.ehviewer.ui.settings

import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.ehviewer.core.database.model.DownloadRecoveryCandidate
import com.ehviewer.core.database.model.DownloadRecoverySourceKind
import com.ehviewer.core.database.model.DownloadRecoveryStatus
import com.ehviewer.core.i18n.R
import com.ehviewer.core.model.BaseGalleryInfo
import com.hippo.ehviewer.EhDB
import com.hippo.ehviewer.collectAsState
import com.hippo.ehviewer.download.downloadLocation
import com.hippo.ehviewer.download.recovery.DownloadRecoveryEhLookup
import com.hippo.ehviewer.download.recovery.RecoveryLookupResult
import com.hippo.ehviewer.ui.Screen
import com.hippo.ehviewer.ui.destinations.ReaderScreenDestination
import com.hippo.ehviewer.ui.reader.ReaderScreenArgs
import com.ramcosta.composedestinations.annotation.Destination
import com.ramcosta.composedestinations.annotation.RootGraph
import com.ramcosta.composedestinations.navigation.DestinationsNavigator
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Destination<RootGraph>
@Composable
fun AnimatedVisibilityScope.DownloadRecoveryCandidatesScreen(
  navigator: DestinationsNavigator,
  autoLookup: Boolean = false,
) =
  Screen(navigator) {
    val candidates by EhDB.downloadRecoveryCandidates.collectAsState(initial = emptyList())
    val scope = rememberCoroutineScope()
    var busyIds by remember { mutableStateOf(emptySet<Long>()) }
    var choiceCandidate by remember { mutableStateOf<DownloadRecoveryCandidate?>(null) }
    var choices by remember { mutableStateOf(emptyList<BaseGalleryInfo>()) }
    var message by remember { mutableStateOf<String?>(null) }
    var autoStarted by rememberSaveable { mutableStateOf(false) }
    val notFoundText = stringResource(R.string.download_recovery_no_match)
    val failedText = stringResource(R.string.download_recovery_fetch_failed)
    val recoveredText = stringResource(R.string.download_recovery_item_recovered)

    suspend fun performLookup(candidate: DownloadRecoveryCandidate): Boolean {
      busyIds = busyIds + candidate.candidateId
      return try {
        when (val result = DownloadRecoveryEhLookup.lookup(candidate)) {
          is RecoveryLookupResult.Choices -> {
            choiceCandidate = candidate
            choices = result.galleries
            false
          }
          RecoveryLookupResult.NotFound -> {
            DownloadRecoveryEhLookup.markNotFound(candidate)
            message = notFoundText
            true
          }
        }
      } catch (cancelled: CancellationException) {
        throw cancelled
      } catch (failure: Exception) {
        DownloadRecoveryEhLookup.markFailure(candidate, failure)
        message = failedText
        true
      } finally {
        busyIds = busyIds - candidate.candidateId
      }
    }

    LaunchedEffect(autoLookup, candidates.size) {
      if (autoLookup && !autoStarted && candidates.isNotEmpty()) {
        autoStarted = true
        for (candidate in
          candidates.filter { it.status == DownloadRecoveryStatus.MISSING_IDENTITY }) {
          if (!performLookup(candidate)) break
        }
      }
    }

    Column(modifier = Modifier.fillMaxSize()) {
      Text(
        text = stringResource(R.string.download_recovery_candidates_title),
        modifier = Modifier.padding(20.dp),
        style = MaterialTheme.typography.headlineSmall,
      )
      message?.let {
        Text(
          text = it,
          modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
          style = MaterialTheme.typography.bodyMedium,
        )
      }
      if (candidates.isEmpty()) {
        Text(
          text = stringResource(R.string.download_recovery_empty),
          modifier = Modifier.padding(20.dp),
        )
      } else {
        LazyColumn(
          modifier = Modifier.fillMaxSize(),
          contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
          verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
          items(candidates, key = DownloadRecoveryCandidate::candidateId) { candidate ->
            Card(modifier = Modifier.fillMaxWidth()) {
              Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
              ) {
                Text(
                  candidate.title ?: candidate.displayName,
                  style = MaterialTheme.typography.titleMedium,
                )
                Text(candidate.relativePath, style = MaterialTheme.typography.bodySmall)
                Text(
                  stringResource(R.string.download_recovery_status, candidate.status.name),
                  style = MaterialTheme.typography.bodySmall,
                )
                if (candidate.issues.isNotBlank()) {
                  Text(candidate.issues, style = MaterialTheme.typography.bodySmall)
                }
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                  OutlinedButton(
                    enabled =
                      candidate.localPageCount > 0 &&
                        candidate.status != DownloadRecoveryStatus.SOURCE_MISSING,
                    onClick = {
                      val path = (downloadLocation / candidate.relativePath).toString()
                      val args =
                        if (candidate.sourceKind == DownloadRecoverySourceKind.DIRECTORY) {
                          ReaderScreenArgs.Directory(path, candidate.title ?: candidate.displayName)
                        } else {
                          ReaderScreenArgs.Archive(path)
                        }
                      navigator.navigate(ReaderScreenDestination(args)) { launchSingleTop = true }
                    },
                  ) {
                    Text(stringResource(R.string.download_recovery_open_local))
                  }
                  Button(
                    enabled = candidate.candidateId !in busyIds,
                    onClick = { scope.launch { performLookup(candidate) } },
                  ) {
                    Text(
                      if (candidate.candidateId in busyIds) {
                        stringResource(R.string.download_recovery_fetching)
                      } else {
                        stringResource(R.string.download_recovery_fetch_from_eh)
                      }
                    )
                  }
                  OutlinedButton(
                    enabled = candidate.candidateId !in busyIds,
                    onClick = {
                      scope.launch { EhDB.removeDownloadRecoveryCandidate(candidate.candidateId) }
                    },
                  ) {
                    Text(stringResource(R.string.download_recovery_remove))
                  }
                }
              }
            }
          }
        }
      }
    }

    val selectedCandidate = choiceCandidate
    if (selectedCandidate != null && choices.isNotEmpty()) {
      AlertDialog(
        onDismissRequest = {
          choiceCandidate = null
          choices = emptyList()
        },
        title = { Text(stringResource(R.string.download_recovery_choose_match)) },
        text = {
          LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(choices, key = BaseGalleryInfo::gid) { gallery ->
              OutlinedButton(
                modifier = Modifier.fillMaxWidth(),
                onClick = {
                  choiceCandidate = null
                  choices = emptyList()
                  scope.launch {
                    try {
                      val promoted = DownloadRecoveryEhLookup.apply(selectedCandidate, gallery)
                      message = if (promoted) recoveredText else failedText
                    } catch (cancelled: CancellationException) {
                      throw cancelled
                    } catch (failure: Exception) {
                      DownloadRecoveryEhLookup.markFailure(selectedCandidate, failure)
                      message = failedText
                    }
                  }
                },
              ) {
                Column(modifier = Modifier.fillMaxWidth()) {
                  Text(gallery.title.orEmpty())
                  Text("gid: ${gallery.gid}", style = MaterialTheme.typography.bodySmall)
                }
              }
            }
          }
        },
        confirmButton = {},
        dismissButton = {
          OutlinedButton(
            onClick = {
              choiceCandidate = null
              choices = emptyList()
            }
          ) {
            Text(stringResource(R.string.download_recovery_exit))
          }
        },
      )
    }
  }
