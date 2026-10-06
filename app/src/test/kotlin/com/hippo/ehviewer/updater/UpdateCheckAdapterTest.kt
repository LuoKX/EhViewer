package com.hippo.ehviewer.updater

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.*
import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class UpdateCheckAdapterTest {
  init {
    UpdaterHostLibrary.load()
  }

  private val clock =
    UpdatePolicyRequest(
      operation = UpdateOperation.CHECK,
      nowSeconds = 42,
      lastSeconds = 3,
      forceCheck = true,
    )
  private val variant = { UpdateVariant("default", listOf("arm64-v8a")) }

  @Test
  fun skippedCheckHasNoPersistenceChannelReadHttpOrAbiEffects() = runBlocking {
    val source = OfflineSource()
    assertNull(
      executeUpdateCheck(
        clock.copy(forceCheck = false),
        {
          fail("Skipped channel read")
        },
        "v2",
        "v2",
        {
          fail("Skipped persistence")
        },
        source,
        variant = { fail("Skipped ABI capture") },
      )
    )
    assertTrue(source.events.isEmpty())
  }

  @Test
  fun stableUsesStringInequalityAndPersistsBeforeSingleReleaseRequest() = runBlocking {
    val source = OfflineSource()
    val result =
      executeUpdateCheck(
        clock,
        {
          false
        },
        "v2",
        "ignored",
        { source.events.add("persist/$it") },
        source,
        variant,
      )
    assertEquals(Release("v1", "notes", "first"), result)
    assertEquals(listOf("persist/42", "release"), source.events)
  }

  @Test
  fun sameStableVersionIsNoUpdateButEmptyAssetsStillThrowRepairedError() = runBlocking {
    val source = OfflineSource()
    assertNull(executeUpdateCheck(clock, { false }, "v1", "ignored", {}, source, variant))
    assertEquals(listOf("release"), source.events)
    source.releaseJson = json("release_empty")
    assertEquals(
      "No update release assets available",
      assertFailsWith<IOException> {
          executeUpdateCheck(clock, { false }, "v2", "ignored", {}, source, variant)
        }
        .message,
    )
    assertEquals(listOf("release", "release"), source.events)
  }

  @Test
  fun ciStagesKeepOrderExpiredArtifactBehaviorAndKotlinChangelogFormatting() = runBlocking {
    val source = OfflineSource()
    val result =
      executeUpdateCheck(
        clock,
        {
          true
        },
        "ignored",
        "v2",
        { source.events.add("persist/$it") },
        source,
        variant,
      )
    assertEquals(Release("abcdef0", "title (@author)", "first"), result)
    assertEquals(
      listOf("persist/42", "repo", "workflow/main", "artifacts", "comparison/v2/abcdef0"),
      source.events,
    )
  }

  @Test
  fun sameShaStopsBeforeArtifactsComparisonAndAbiCapture() = runBlocking {
    val source = OfflineSource()
    source.artifactJson = "bad"
    assertNull(
      executeUpdateCheck(
        clock,
        {
          true
        },
        "ignored",
        "abcdef0",
        {},
        source,
        variant = { fail("Same SHA ABI captured") },
      )
    )
    assertEquals(listOf("repo", "workflow/main"), source.events)
  }

  @Test
  fun emptyWorkflowOrArtifactsFailAfterPersistenceWithoutNoUpdateOrRetry() = runBlocking {
    for ((fixture, message, events) in
      listOf(
        Triple(
          "workflow_empty",
          "No successful update workflow runs available",
          listOf("persist/42", "repo", "workflow/main"),
        ),
        Triple(
          "artifacts_empty",
          "No update workflow artifacts available",
          listOf("persist/42", "repo", "workflow/main", "artifacts"),
        ),
      )) {
      val source = OfflineSource()
      if (fixture == "workflow_empty") {
        source.workflowJson = json(fixture)
      } else {
        source.artifactJson = json(fixture)
      }
      assertEquals(
        message,
        assertFailsWith<IOException> {
            executeUpdateCheck(
              clock,
              {
                true
              },
              "ignored",
              "v2",
              { source.events.add("persist/$it") },
              source,
              variant,
            )
          }
          .message,
      )
      assertEquals(events, source.events)
    }
  }

  @Test
  fun comparisonFailureRetainsWorkflowTitleAndCancellationStillPropagates() = runBlocking {
    val source = OfflineSource()
    source.comparisonJson = "malformed-secret-response"
    assertEquals(
      "workflow title",
      executeUpdateCheck(
          clock,
          {
            true
          },
          "ignored",
          "v2",
          {},
          source,
          variant,
        )
        ?.changelog,
    )
    val cancellation = CancellationException("fixture")
    source.comparisonFailure = cancellation
    try {
      executeUpdateCheck(clock, { true }, "ignored", "v2", {}, source, variant)
      fail("Cancellation hidden")
    } catch (actual: CancellationException) {
      assertSame(cancellation, actual)
    }
  }

  @Test
  fun rejectedInitialEnvelopeAndLinkageFailureHaveNoPlatformEffectsOrFallback() = runBlocking {
    for (linkage in listOf(false, true)) {
      val source = OfflineSource()
      var calls = 0
      val failure = LinkageError("fixture")
      try {
        executeUpdateCheck(
          clock,
          {
            fail("Unvalidated channel read")
          },
          "v2",
          "v2",
          { fail("Unvalidated persistence") },
          source,
          variant,
          planner = { request ->
            planUpdate(request) { _, _, output ->
              calls++
              if (linkage) throw failure
              val bytes =
                rustCbor.encodeToByteArray(
                  ApiEnvelope<UpdatePlan>(
                    2,
                    "updater",
                    error = RustError(RustErrorKind.ERROR, "Invalid update response"),
                  )
                )
              output.put(bytes)
              bytes.size
            }
          },
        )
        fail("Invalid native result hidden")
      } catch (actual: Throwable) {
        if (linkage) assertSame(failure, actual) else assertIs<IllegalArgumentException>(actual)
      }
      assertEquals(1, calls)
      assertTrue(source.events.isEmpty())
    }
  }

  @Test
  fun rejectedArtifactPlanStopsBeforeComparisonWithoutFallback() = runBlocking {
    val source = OfflineSource()
    var artifactCalls = 0
    assertFailsWith<IllegalArgumentException> {
      executeUpdateCheck(
        clock,
        { true },
        "ignored",
        "v2",
        {},
        source,
        variant,
        planner = { request ->
          if (request.operation == UpdateResponseOperation.ARTIFACTS) {
            val response =
              rustCbor.decodeFromByteArray<ApiEnvelope<UpdatePlan>>(
                UpdaterHostLibrary.golden("artifacts_response")
              )
            val wrong = response.copy(payload = response.payload!!.copy(selectedIndex = 3))
            planUpdate(request) { _, _, output ->
              artifactCalls++
              val bytes = rustCbor.encodeToByteArray(wrong)
              output.put(bytes)
              bytes.size
            }
          } else {
            planUpdate(request)
          }
        },
      )
    }
    assertEquals(1, artifactCalls)
    assertEquals(listOf("repo", "workflow/main", "artifacts"), source.events)
  }

  private class OfflineSource : UpdateResponseSource {
    val events = mutableListOf<String>()
    var releaseJson = json("release")
    var workflowJson = json("workflow")
    var artifactJson = json("artifacts")
    var comparisonJson = json("comparison")
    var comparisonFailure: Throwable? = null

    override suspend fun repository(): String {
      events.add("repo")
      return json("repo")
    }

    override suspend fun workflow(branch: String): String {
      events.add("workflow/$branch")
      return workflowJson
    }

    override suspend fun artifacts(url: String): String {
      assertEquals("artifacts-endpoint", url)
      events.add("artifacts")
      return artifactJson
    }

    override suspend fun comparison(
      current: String,
      latest: String,
    ): String {
      events.add("comparison/$current/$latest")
      comparisonFailure?.let { throw it }
      return comparisonJson
    }

    override suspend fun release(): String {
      events.add("release")
      return releaseJson
    }
  }

  companion object {
    private fun json(name: String) =
      checkNotNull(
        rustCbor
          .decodeFromByteArray<UpdateRequest>(UpdaterHostLibrary.golden("${name}_request"))
          .json
      )
  }
}
