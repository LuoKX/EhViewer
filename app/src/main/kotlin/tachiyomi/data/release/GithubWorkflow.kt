package tachiyomi.data.release

import com.hippo.ehviewer.bridge.updater.UpdateOperation
import com.hippo.ehviewer.bridge.updater.UpdatePolicyRequest
import com.hippo.ehviewer.bridge.updater.planUpdatePolicy
import com.hippo.ehviewer.bridge.updater.selectUpdateBinary
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable data class GithubRepo(@SerialName("default_branch") val defaultBranch: String)

@Serializable
data class GithubWorkflowRuns(
  @SerialName("workflow_runs") val workflowRuns: List<GithubWorkflowRun>
) {
  fun latestRun(): GithubWorkflowRun {
    val snapshot = workflowRuns.toList()
    val plan =
      planUpdatePolicy(
        UpdatePolicyRequest(
          operation = UpdateOperation.SELECT_WORKFLOW,
          names = snapshot.map { it.headSha },
        )
      )
    return snapshot[checkNotNull(plan.index)]
  }
}

@Serializable
data class GithubWorkflowRun(
  @SerialName("head_sha") val headSha: String,
  @SerialName("display_title") val title: String,
  @SerialName("artifacts_url") val artifactsUrl: String,
)

@Serializable
data class GithubArtifacts(@SerialName("artifacts") val artifacts: List<GithubArtifact>) {
  fun getDownloadLink(matchesVariant: ((String) -> Boolean)? = null): String {
    val snapshot = artifacts.toList()
    val index = selectUpdateBinary(snapshot.map { it.name }, true, matchesVariant)
    return snapshot[index].downloadLink
  }
}

@Serializable
data class GithubArtifact(
  val name: String,
  @SerialName("archive_download_url") val downloadLink: String,
)
