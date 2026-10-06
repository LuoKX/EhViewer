package com.hippo.ehviewer.bridge.updater

import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import tachiyomi.data.release.GitHubAssets
import tachiyomi.data.release.GithubArtifact
import tachiyomi.data.release.GithubArtifacts
import tachiyomi.data.release.GithubRelease

class UpdateSelectionAdapterTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun customCallbacksAreLazyAndKeepServerVersusStableUtf16Order() {
    val names = listOf("\ue000", "😀", "😀", "a")
    for (artifacts in listOf(false, true)) {
      val expectedOrder = if (artifacts) listOf(3, 1, 2, 0) else names.indices.toList()
      for (matchAt in listOf(0, 1, 3, -1)) {
        val visited = mutableListOf<String>()
        var calls = 0
        val selected =
          selectUpdateBinary(
            names,
            artifacts,
            { name ->
              visited.add(name)
              visited.lastIndex == matchAt
            },
            variant = { error("Custom callback must not read platform ABI") },
            planner = { request ->
              calls++
              planUpdatePolicy(request)
            },
          )
        val visitedIndices = if (matchAt < 0) expectedOrder else expectedOrder.take(matchAt + 1)
        assertEquals(visitedIndices.map { names[it] }, visited)
        assertEquals(if (matchAt < 0) expectedOrder[0] else expectedOrder[matchAt], selected)
        assertEquals(2, calls)
      }
    }
  }

  @Test
  fun callbackExceptionsAndCancellationPropagateWithoutLaterCallbacksOrFinalPlan() {
    for (failure in listOf(IllegalStateException("fixture"), CancellationException("fixture"))) {
      var callbacks = 0
      var plans = 0
      try {
        selectUpdateBinary(
          listOf("z", "a", "m"),
          true,
          {
            callbacks++
            if (callbacks == 2) throw failure
            false
          },
          planner = {
            plans++
            planUpdatePolicy(it)
          },
        )
        fail("Callback failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(2, callbacks)
      assertEquals(1, plans)
    }
  }

  @Test
  fun emptyCollectionsUseOneErrorPlanWithZeroCallbacksAndDefaultUsesOnePlan() {
    for (artifacts in listOf(false, true)) {
      var calls = 0
      val failure =
        assertFailsWith<IOException> {
          selectUpdateBinary(
            emptyList(),
            artifacts,
            {
              fail("Empty callback invoked")
            },
            variant = { fail("Empty ABI captured") },
            planner = {
              calls++
              planUpdatePolicy(it)
            },
          )
        }
      assertEquals(
        if (artifacts) "No update workflow artifacts available"
        else "No update release assets available",
        failure.message,
      )
      assertEquals(1, calls)
      calls = 0
      assertEquals(
        1,
        selectUpdateBinary(
          listOf("z-other", "a-default-x86_64"),
          artifacts,
          null,
          variant = {
            UpdateVariant("default", listOf("x86_64", "arm64-v8a"))
          },
          planner = {
            calls++
            planUpdatePolicy(it)
          },
        ),
      )
      assertEquals(1, calls)
    }
  }

  @Test
  fun malformedOrderNeverInvokesCallbacksAndWrongFinalSelectionFails() {
    val names = listOf("z", "a")
    val good = UpdatePolicyPlan(UpdateDecision.OBSERVE, null, null, null, listOf(1, 0))
    for (wrong in
      listOf(
        good.copy(order = listOf(0, 0)),
        good.copy(order = listOf(1)),
        good.copy(order = listOf(-1, 0)),
        good.copy(order = listOf(1, 2)),
        good.copy(order = listOf(0, 1)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        selectUpdateBinary(
          names,
          true,
          { fail("Invalid plan invoked callback") },
          planner = { wrong },
        )
      }
    }
    var plans = 0
    assertFailsWith<IllegalArgumentException> {
      selectUpdateBinary(
        names,
        true,
        { true },
        planner = {
          plans++
          if (plans == 1) good else UpdatePolicyPlan(UpdateDecision.SELECTED, 0, null, null, null)
        },
      )
    }
    assertEquals(2, plans)
  }

  @Test
  fun publicHelpersKeepSnapshotAgainstCallbackMutationAndStableDuplicateFallback() {
    val assets = mutableListOf(GitHubAssets("first", "z"), GitHubAssets("second", "a"))
    val release = GithubRelease("v1", "", "", assets)
    assertEquals(
      "second",
      release.getDownloadLink {
        assets.clear()
        it == "a"
      },
    )
    val artifacts =
      mutableListOf(
        GithubArtifact("😀", "first"),
        GithubArtifact("\ue000", "last"),
        GithubArtifact("😀", "duplicate"),
      )
    val value = GithubArtifacts(artifacts)
    assertEquals(
      "first",
      value.getDownloadLink {
        artifacts.clear()
        false
      },
    )
  }
}
