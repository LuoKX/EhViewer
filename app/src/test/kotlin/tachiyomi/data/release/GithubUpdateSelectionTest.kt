package tachiyomi.data.release

import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class GithubUpdateSelectionTest {
  @Test
  fun emptyResponsesFailWithoutInspectingNames() {
    val matcher: (String) -> Boolean = { error("Empty collection must not inspect a name") }
    assertEquals(
      "No update release assets available",
      assertFailsWith<IOException> {
          release(emptyList()).getDownloadLink(matcher)
        }
        .message,
    )
    assertEquals(
      "No update workflow artifacts available",
      assertFailsWith<IOException> {
          GithubArtifacts(emptyList()).getDownloadLink(matcher)
        }
        .message,
    )
    assertEquals(
      "No successful update workflow runs available",
      assertFailsWith<IOException> {
          GithubWorkflowRuns(emptyList()).latestRun()
        }
        .message,
    )
  }

  @Test
  fun releasePreservesFirstMatchAndOriginalOrderFallback() {
    val value = release(listOf(GitHubAssets("first", "z-arm"), GitHubAssets("second", "a-arm")))
    assertEquals("first", value.getDownloadLink { it.endsWith("arm") })
    assertEquals("second", value.getDownloadLink { it.startsWith("a") })
    assertEquals("first", value.getDownloadLink { false })
  }

  @Test
  fun artifactsPreserveSortedMatchAndSortedFallback() {
    val value =
      GithubArtifacts(listOf(GithubArtifact("z-arm", "last"), GithubArtifact("a-arm", "first")))
    assertEquals("first", value.getDownloadLink { it.endsWith("arm") })
    assertEquals("last", value.getDownloadLink { it.startsWith("z") })
    assertEquals("first", value.getDownloadLink { false })
  }

  @Test
  fun workflowUsesFirstServerOrderedRun() {
    val first = GithubWorkflowRun("abcdef0", "first", "artifacts-first")
    val second = GithubWorkflowRun("abcdef1", "second", "artifacts-second")
    assertEquals(first, GithubWorkflowRuns(listOf(first, second)).latestRun())
  }

  private fun release(assets: List<GitHubAssets>) = GithubRelease("v1", "notes", "release", assets)
}
