package com.hippo.ehviewer.updater

import com.hippo.ehviewer.bridge.updater.UpdateDecision
import com.hippo.ehviewer.bridge.updater.UpdatePlan
import com.hippo.ehviewer.bridge.updater.UpdatePolicyRequest
import com.hippo.ehviewer.bridge.updater.UpdateRequest
import com.hippo.ehviewer.bridge.updater.UpdateResponseOperation
import com.hippo.ehviewer.bridge.updater.UpdateVariant
import com.hippo.ehviewer.bridge.updater.facts
import com.hippo.ehviewer.bridge.updater.installedUpdateVariant
import com.hippo.ehviewer.bridge.updater.planUpdate
import moe.tarsin.coroutines.runSuspendCatching

/** HTTP/auth/cancellation stay in implementations; tests supply offline received JSON only. */
internal interface UpdateResponseSource {
  suspend fun repository(): String

  suspend fun workflow(branch: String): String

  suspend fun artifacts(url: String): String

  suspend fun comparison(
    current: String,
    latest: String,
  ): String

  suspend fun release(): String
}

/** Execute validated plans once, retaining original persistence and changelog failure ordering. */
internal suspend fun executeUpdateCheck(
  clockFacts: UpdatePolicyRequest,
  ciChannel: () -> Boolean,
  currentVersion: String,
  currentSha: String,
  persistLastCheck: (Long) -> Unit,
  source: UpdateResponseSource,
  variant: () -> UpdateVariant = ::installedUpdateVariant,
  planner: (UpdateRequest) -> UpdatePlan = ::planUpdate,
): Release? {
  val eligibility =
    planner(UpdateRequest(operation = UpdateResponseOperation.POLICY, policy = clockFacts))
  if (checkNotNull(eligibility.policy).decision == UpdateDecision.SKIP) return null
  persistLastCheck(clockFacts.nowSeconds)
  if (ciChannel()) {
    val facts = clockFacts.copy(current = currentSha)
    val repository =
      planner(
        UpdateRequest(
          operation = UpdateResponseOperation.REPO,
          policy = facts,
          json = source.repository(),
        )
      )
    val workflow =
      planner(
        UpdateRequest(
          operation = UpdateResponseOperation.WORKFLOW,
          policy = facts,
          json = source.workflow(checkNotNull(repository.branch)),
        )
      )
    if (checkNotNull(workflow.policy).decision == UpdateDecision.NO_UPDATE) return null
    val run = checkNotNull(workflow.workflowRun)
    val shortSha = checkNotNull(workflow.policy.version)
    val artifactJson = source.artifacts(run.artifactsUrl)
    val artifacts =
      planner(
        UpdateRequest(
          operation = UpdateResponseOperation.ARTIFACTS,
          policy = variant().facts(),
          json = artifactJson,
        )
      )
    val changelog = runSuspendCatching {
      val comparison =
        planner(
          UpdateRequest(
            operation = UpdateResponseOperation.COMPARISON,
            policy = facts,
            json = source.comparison(currentSha, shortSha),
          )
        )
      checkNotNull(comparison.commits).joinToString("\n") { commit ->
        "${commit.commit.message.takeWhile { it != '\n' }} (@${commit.commit.author.name})"
      }
    }
      .getOrDefault(run.title)
    return Release(shortSha, changelog, checkNotNull(artifacts.downloadLink))
  }
  val releaseJson = source.release()
  val release =
    planner(
      UpdateRequest(
        operation = UpdateResponseOperation.RELEASE,
        policy = variant().facts().copy(current = currentVersion),
        json = releaseJson,
      )
    )
  if (checkNotNull(release.policy).decision == UpdateDecision.NO_UPDATE) return null
  return Release(
    checkNotNull(release.policy.version),
    checkNotNull(release.info),
    checkNotNull(release.downloadLink),
  )
}
