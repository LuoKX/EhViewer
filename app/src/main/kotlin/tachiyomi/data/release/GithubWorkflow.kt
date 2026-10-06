package tachiyomi.data.release

import com.hippo.ehviewer.util.AppConfig
import java.io.IOException
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable data class GithubRepo(
  @SerialName("default_branch") val defaultBranch: String,
)

@Serializable
data class GithubWorkflowRuns(
  @SerialName("workflow_runs") val workflowRuns: List<GithubWorkflowRun>,
) {
  fun latestRun(): GithubWorkflowRun =
    workflowRuns.firstOrNull() ?: throw IOException("No successful update workflow runs available")
}

@Serializable
data class GithubWorkflowRun(
  @SerialName("head_sha") val headSha: String,
  @SerialName("display_title") val title: String,
  @SerialName("artifacts_url") val artifactsUrl: String,
)

@Serializable
data class GithubArtifacts(
  @SerialName("artifacts") val artifacts: List<GithubArtifact>,
) {
  fun getDownloadLink(
    matchesVariant: (String) -> Boolean = { AppConfig.matchVariant(it) },
  ): String {
    // The default order is upload order, so we need to sort it.
    val ordered = artifacts.sortedBy { it.name }
    val fallback =
      ordered.firstOrNull() ?: throw IOException("No update workflow artifacts available")
    return (ordered.find { matchesVariant(it.name) } ?: fallback).downloadLink
  }
}

@Serializable
data class GithubArtifact(
  val name: String,
  @SerialName("archive_download_url") val downloadLink: String,
)
