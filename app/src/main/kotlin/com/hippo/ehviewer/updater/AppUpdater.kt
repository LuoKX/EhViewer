package com.hippo.ehviewer.updater

import com.ehviewer.core.files.write
import com.hippo.ehviewer.BuildConfig
import com.hippo.ehviewer.EhApplication.Companion.ktorClient
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.updater.UPDATE_MAX_JSON
import com.hippo.ehviewer.bridge.updater.UpdateOperation
import com.hippo.ehviewer.bridge.updater.UpdatePolicyRequest
import com.hippo.ehviewer.client.executeSafely
import com.hippo.ehviewer.spider.timeoutBySpeed
import com.hippo.ehviewer.util.copyTo
import com.hippo.ehviewer.util.ensureSuccess
import com.hippo.ehviewer.util.readToBuffer
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.client.request.accept
import io.ktor.client.request.bearerAuth
import io.ktor.client.request.prepareGet
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.ContentType
import io.ktor.utils.io.jvm.javaio.toInputStream
import java.nio.ByteBuffer
import java.util.zip.ZipInputStream
import kotlin.time.Clock
import kotlinx.io.asSource
import okio.Path

private const val API_URL = "https://api.github.com/repos/${BuildConfig.REPO_NAME}"
private const val LATEST_RELEASE_URL = "$API_URL/releases/latest"

object AppUpdater {
  suspend fun checkForUpdate(forceCheck: Boolean = false): Release? {
    val now = Clock.System.now()
    return executeUpdateCheck(
      clockFacts =
        UpdatePolicyRequest(
          operation = UpdateOperation.CHECK,
          nowSeconds = now.epochSeconds,
          nowNanos = now.nanosecondsOfSecond,
          lastSeconds = Settings.lastUpdateTime,
          intervalDays = Settings.updateIntervalDays.value,
          forceCheck = forceCheck,
        ),
      ciChannel = { Settings.useCIUpdateChannel.value },
      currentVersion = BuildConfig.RAW_VERSION_NAME,
      currentSha = BuildConfig.COMMIT_SHA,
      persistLastCheck = { Settings.lastUpdateTime = it },
      source = GithubUpdateResponseSource,
    )
  }

  suspend fun downloadUpdate(
    url: String,
    path: Path,
  ) {
    val isZip = url.endsWith("zip")
    timeoutBySpeed(
      url,
      {
        ghStatement(url) {
          // https://docs.github.com/en/rest/releases/assets?apiVersion=2022-11-28#get-a-release-asset
          if (!isZip) accept(ContentType.Application.OctetStream)
          it()
        }
      },
      { _, _, _ -> },
      { response ->
        if (isZip) {
          response.bodyAsChannel().toInputStream().use { stream ->
            ZipInputStream(stream).use { zip ->
              zip.nextEntry
              path.write { transferFrom(zip.asSource()) }
            }
          }
        } else {
          response.bodyAsChannel().copyTo(path)
        }
      },
    )
  }
}

private object GithubUpdateResponseSource : UpdateResponseSource {
  override suspend fun repository() = ghJsonResponse(API_URL)

  override suspend fun workflow(branch: String) =
    ghJsonResponse(
      "$API_URL/actions/workflows/ci.yml/runs?branch=$branch&event=push&status=success&per_page=1"
    )

  override suspend fun artifacts(url: String) = ghJsonResponse(url)

  override suspend fun comparison(current: String, latest: String) =
    ghJsonResponse("$API_URL/compare/$current...$latest")

  override suspend fun release() = ghJsonResponse(LATEST_RELEASE_URL)
}

private suspend fun ghJsonResponse(url: String) =
  ghStatement(url).executeSafely { response ->
    response.status.ensureSuccess()
    val buffer = ByteBuffer.allocate(UPDATE_MAX_JSON)
    response.bodyAsChannel().readToBuffer(buffer)
    buffer.flip()
    // Same replacement behavior as bodyAsUtf8Text, with bounded network input.
    Charsets.UTF_8.decode(buffer).toString()
  }

private suspend inline fun ghStatement(
  url: String,
  builder: HttpRequestBuilder.() -> Unit = {},
) =
  ktorClient.prepareGet(url) {
    bearerAuth(GithubTokenParts.joinToString("_"))
    apply(builder)
  }

private val GithubTokenParts =
  arrayOf(
    "github",
    "pat",
    "11A4H2ACI0iGDuL1O6wPYW",
    "OTFg8xaCNUwR1NHaJE1AT3LoYPfz6bouI7E7ReLf8GjIRFHCL5UsHL9EnWP",
  )

data class Release(
  val version: String,
  val changelog: String,
  val downloadLink: String,
)
