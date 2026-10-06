package tachiyomi.data.release

import com.hippo.ehviewer.util.AppConfig
import java.io.IOException
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Contains information about the latest release from GitHub. */
@Serializable
data class GithubRelease(
  @SerialName("tag_name") val version: String,
  @SerialName("body") val info: String,
  @SerialName("html_url") val releaseLink: String,
  @SerialName("assets") val assets: List<GitHubAssets>,
) {
  fun getDownloadLink(
    matchesVariant: (String) -> Boolean = { AppConfig.matchVariant(it) },
  ): String {
    val fallback = assets.firstOrNull() ?: throw IOException("No update release assets available")
    return (assets.find { matchesVariant(it.name) } ?: fallback).url
  }
}

/** Assets class containing download url. */
@Serializable data class GitHubAssets(
  val url: String,
  val name: String,
)
