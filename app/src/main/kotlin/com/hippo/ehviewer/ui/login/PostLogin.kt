package com.hippo.ehviewer.ui.login

import com.ehviewer.core.network.EhCookieStore
import com.ehviewer.core.util.logcat
import com.hippo.ehviewer.bridge.auth.SessionGallerySite
import com.hippo.ehviewer.client.EhEngine
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.session.SessionAttempt
import com.hippo.ehviewer.client.session.SessionAuthentication
import com.hippo.ehviewer.client.session.SessionProfile
import com.hippo.ehviewer.client.session.sessionController
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch

/** Refreshes account presentation without allowing a stale response to outlive logout. */
suspend fun refreshAccountInfo() {
  try {
    sessionController.refreshProfile {
      EhEngine.getProfile().let { SessionProfile(it.displayName, it.avatar) }
    }
  } catch (error: CancellationException) {
    throw error
  } catch (error: Exception) {
    logcat("PostLogin") { "Account profile refresh failed: ${error::class.simpleName}" }
  }
}

/**
 * Completes one password or WebView authentication through the serialized session controller.
 *
 * Profile, news, and configuration requests remain Android-owned and structured under the caller.
 * The returned value is `false` when a newer session transition superseded [attempt].
 */
internal suspend fun postLogin(attempt: SessionAttempt, username: String? = null): Boolean =
  sessionController.finishSignIn(attempt) { loadAuthentication(username) }

private suspend fun loadAuthentication(username: String?): SessionAuthentication = coroutineScope {
  val profile = async {
    bestEffort {
      EhEngine.getProfile().let { SessionProfile(it.displayName, it.avatar) }
    }
  }
  launch {
    // Refreshes the `star` cookie used by the existing Android account flows.
    bestEffort { EhEngine.getNews(false) }
  }
  launch {
    // Refreshes public-site cookies used for image limits.
    bestEffort {
      EhEngine.getUConfig(EhUrl.SITE_E)
      EhCookieStore.flush()
    }
  }
  val site = async {
    try {
      EhEngine.getUConfig(EhUrl.SITE_EX)
      EhCookieStore.flush()
      SessionGallerySite.EX_HENTAI
    } catch (error: CancellationException) {
      throw error
    } catch (_: Exception) {
      SessionGallerySite.PUBLIC
    }
  }

  SessionAuthentication(username, profile.await(), site.await())
}

private suspend fun <T> bestEffort(operation: suspend () -> T): T? =
  try {
    operation()
  } catch (error: CancellationException) {
    throw error
  } catch (error: Exception) {
    logcat("PostLogin") { "Session follow-up failed: ${error::class.simpleName}" }
    null
  }
