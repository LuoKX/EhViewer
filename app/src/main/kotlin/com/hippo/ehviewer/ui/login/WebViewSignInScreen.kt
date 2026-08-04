package com.hippo.ehviewer.ui.login

import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import com.ehviewer.core.network.EhCookieStore
import com.ehviewer.core.util.logcat
import com.google.accompanist.web.WebView
import com.google.accompanist.web.rememberWebViewState
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.client.EhUrl
import com.hippo.ehviewer.client.session.SessionAttempt
import com.hippo.ehviewer.client.session.sessionController
import com.hippo.ehviewer.ui.Screen
import com.hippo.ehviewer.util.setDefaultSettings
import com.ramcosta.composedestinations.annotation.Destination
import com.ramcosta.composedestinations.annotation.RootGraph
import com.ramcosta.composedestinations.navigation.DestinationsNavigator
import kotlinx.coroutines.CancellationException

@Destination<RootGraph>
@Composable
fun AnimatedVisibilityScope.WebViewSignInScreen(navigator: DestinationsNavigator) =
  Screen(navigator) {
    val state = rememberWebViewState(url = EhUrl.URL_SIGN_IN)
    var attempt by remember { mutableStateOf<SessionAttempt?>(null) }
    var completionInFlight by remember { mutableStateOf(false) }
    LaunchedEffect(state) {
      val activeAttempt = sessionController.beginSignIn()
      attempt = activeAttempt
      fun stopAfter(error: Throwable) {
        logcat("WebViewSignIn") { "Session handoff failed: ${error::class.simpleName}" }
        state.webView?.destroy()
        navigator.popBackStack()
      }
      snapshotFlow { !state.isLoading }
        .collect { hasFinished ->
          if (hasFinished && !completionInFlight) {
            if (EhCookieStore.isCloudflareBypassed()) {
              Settings.desktopSite.value = false
            }
            if (EhCookieStore.hasSignedIn()) {
              completionInFlight = true
              try {
                EhCookieStore.flush()
                if (postLogin(activeAttempt)) {
                  state.webView?.destroy()
                } else {
                  // A newer logout/login owns the session; this WebView must not retry.
                  state.webView?.destroy()
                  navigator.popBackStack()
                }
              } catch (error: CancellationException) {
                throw error
              } catch (error: Exception) {
                stopAfter(error)
              } catch (error: LinkageError) {
                stopAfter(error)
              }
            }
          }
        }
    }
    if (attempt != null) {
      WebView(
        state = state,
        modifier = Modifier.fillMaxSize(),
        onCreated = { it.setDefaultSettings() },
      )
    }
  }
