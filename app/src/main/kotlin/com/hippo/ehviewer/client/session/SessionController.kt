package com.hippo.ehviewer.client.session

import com.ehviewer.core.network.EhCookieStore
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.CoreBackend
import com.hippo.ehviewer.bridge.CoreBackendSelector
import com.hippo.ehviewer.bridge.auth.SessionGallerySite
import com.hippo.ehviewer.bridge.auth.SessionLifecycleCommand
import com.hippo.ehviewer.bridge.auth.SessionLifecycleMode
import com.hippo.ehviewer.bridge.auth.SessionLifecyclePlan
import com.hippo.ehviewer.bridge.auth.SessionLifecycleSnapshot
import com.hippo.ehviewer.bridge.auth.SessionProfileUpdateKind
import com.hippo.ehviewer.bridge.auth.planSessionLifecycle
import com.hippo.ehviewer.client.EhUrl
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/** Opaque generation token that authorizes exactly one authentication completion. */
@JvmInline internal value class SessionAttempt internal constructor(val generation: Long)

/** Account presentation data loaded by Android during an authentication handoff. */
internal data class SessionProfile(val displayName: String, val avatar: String?)

/** Sanitized result of Android-owned authentication follow-up requests. */
internal data class SessionAuthentication(
  val username: String?,
  val profile: SessionProfile?,
  val site: SessionGallerySite,
) {
  /** Prefers the profile display name over the identity parsed from a password response. */
  val effectiveUsername: String?
    get() = profile?.displayName ?: username
}

/** Mutable Android state required to snapshot and commit one lifecycle transition. */
internal interface SessionStore {
  /** Returns current persisted flags plus identity-cookie presence. */
  fun snapshot(): SessionLifecycleSnapshot

  /** Awaits complete removal of every WebView cookie. */
  suspend fun clearCookies()

  /** Persists the account display name. */
  fun setDisplayName(displayName: String?)

  /** Persists the account avatar URL. */
  fun setAvatar(avatar: String?)

  /** Persists whether the application has a completed authenticated session. */
  fun setSignedIn(signedIn: Boolean)

  /** Persists whether navigation must present the login choice. */
  fun setSignInRequired(required: Boolean)

  /** Persists the selected remote gallery category. */
  fun setSite(site: SessionGallerySite)
}

/**
 * Serializes startup, login, guest, logout, and profile commits around one generation counter.
 *
 * Remote profile/configuration work runs outside [mutex]. A later transition increments the
 * generation and makes stale work unable to commit. Once cookie removal begins, the small state
 * commit is non-cancellable so persisted flags cannot disagree with an already-cleared WebView.
 */
internal class SessionController(
  private val store: SessionStore,
  private val backendSelector: () -> CoreBackend,
  private val planner: (SessionLifecycleSnapshot, SessionLifecycleCommand) -> SessionLifecyclePlan =
    ::planSessionLifecycle,
) {
  private val mutex = Mutex()
  private var generation = 0L
  private var profileGeneration = 0L

  /** Reconciles persisted flags and cookies before startup navigation is published. */
  suspend fun restore() {
    mutex.withLock {
      advanceGeneration()
      transition(SessionLifecycleCommand.restore(), authentication = null) {
        // The rollout-disabled startup path intentionally preserves legacy persisted state.
      }
    }
  }

  /** Clears the previous identity and creates a one-shot token for a new login handoff. */
  suspend fun beginSignIn(): SessionAttempt = mutex.withLock {
    val attempt = SessionAttempt(advanceGeneration())
    transition(SessionLifecycleCommand.signOut(), authentication = null, ::legacySignOut)
    attempt
  }

  /**
   * Loads Android-owned follow-up data and commits it only if [attempt] is still current.
   *
   * @return `true` when this attempt committed, or `false` when logout or another login superseded
   *   it while [loadAuthentication] was running.
   */
  suspend fun finishSignIn(
    attempt: SessionAttempt,
    loadAuthentication: suspend () -> SessionAuthentication,
  ): Boolean {
    val isCurrent = mutex.withLock { attempt.generation == generation }
    if (!isCurrent) return false

    val authentication = loadAuthentication()
    return mutex.withLock {
      if (attempt.generation != generation) return@withLock false
      // Consume the one-shot token before planning so a partial commit can never be replayed.
      advanceGeneration()
      transition(
        SessionLifecycleCommand.signInSucceeded(
          authentication.effectiveUsername,
          authentication.site,
        ),
        authentication,
      ) {
        legacySignIn(authentication)
      }
      true
    }
  }

  /** Selects anonymous public browsing and invalidates every outstanding login callback. */
  suspend fun enterGuest() {
    mutex.withLock {
      advanceGeneration()
      transition(SessionLifecycleCommand.enterGuest(), authentication = null, ::legacyEnterGuest)
    }
  }

  /** Clears the active identity and invalidates every outstanding login or profile callback. */
  suspend fun signOut() {
    mutex.withLock {
      advanceGeneration()
      transition(SessionLifecycleCommand.signOut(), authentication = null, ::legacySignOut)
    }
  }

  /** Refreshes presentation data only if the same authenticated generation remains active. */
  suspend fun refreshProfile(loadProfile: suspend () -> SessionProfile): Boolean {
    val acceptedGenerations = mutex.withLock {
      if (!store.snapshot().storedSignedIn) return false
      generation to ++profileGeneration
    }
    val profile = loadProfile()
    return mutex.withLock {
      if (
        acceptedGenerations.first != generation ||
          acceptedGenerations.second != profileGeneration ||
          !store.snapshot().storedSignedIn
      ) {
        return@withLock false
      }
      withContext(NonCancellable) {
        store.setDisplayName(profile.displayName)
        store.setAvatar(profile.avatar)
      }
      true
    }
  }

  private suspend fun transition(
    command: SessionLifecycleCommand,
    authentication: SessionAuthentication?,
    legacyCommit: suspend () -> Unit,
  ) {
    val snapshot = store.snapshot()
    val plan =
      when (backendSelector()) {
        CoreBackend.LEGACY -> null
        CoreBackend.RUST -> planner(snapshot, command)
      }
    if (plan == null) {
      withContext(NonCancellable) { legacyCommit() }
    } else {
      withContext(NonCancellable) { commit(plan, authentication) }
    }
  }

  private suspend fun commit(
    plan: SessionLifecyclePlan,
    authentication: SessionAuthentication?,
  ) {
    if (plan.clearCookies) store.clearCookies()
    when (plan.profileUpdate.kind) {
      SessionProfileUpdateKind.KEEP -> Unit
      SessionProfileUpdateKind.SET -> {
        store.setDisplayName(requireNotNull(plan.profileUpdate.username))
        store.setAvatar(authentication?.profile?.avatar)
      }
      SessionProfileUpdateKind.CLEAR -> {
        store.setDisplayName(null)
        store.setAvatar(null)
      }
    }
    publish(plan.state.mode, plan.site)
  }

  private suspend fun legacySignOut() {
    // Frozen rollback behavior clears the display name but historically retains the avatar.
    store.clearCookies()
    store.setDisplayName(null)
    store.setSignedIn(false)
    store.setSite(SessionGallerySite.PUBLIC)
    store.setSignInRequired(true)
  }

  private fun legacySignIn(authentication: SessionAuthentication) {
    authentication.profile?.let { profile ->
      store.setDisplayName(profile.displayName)
      store.setAvatar(profile.avatar)
    }
    store.setSite(authentication.site)
    store.setSignedIn(true)
    store.setSignInRequired(false)
  }

  private fun legacyEnterGuest() {
    // Frozen rollback behavior changes only the public site and navigation requirement.
    store.setSite(SessionGallerySite.PUBLIC)
    store.setSignInRequired(false)
  }

  private fun publish(mode: SessionLifecycleMode, site: SessionGallerySite) {
    store.setSite(site)
    store.setSignedIn(mode == SessionLifecycleMode.AUTHENTICATED)
    store.setSignInRequired(mode == SessionLifecycleMode.SIGN_IN_REQUIRED)
  }

  private fun advanceGeneration(): Long {
    generation++
    profileGeneration++
    return generation
  }
}

/** Process-wide session coordinator used by startup, login, settings, and account UI. */
internal val sessionController =
  SessionController(
    store = AndroidSessionStore,
    backendSelector = CoreBackendSelector::snapshot,
  )

private object AndroidSessionStore : SessionStore {
  override fun snapshot() =
    SessionLifecycleSnapshot(
      hasAuthCookies = EhCookieStore.hasSignedIn(),
      storedSignedIn = Settings.hasSignedIn.value,
      signInRequired = Settings.needSignIn.value,
      username = Settings.displayName.value,
      site = Settings.gallerySite.value.toSessionGallerySite(),
    )

  override suspend fun clearCookies() = EhCookieStore.removeAllCookies()

  override fun setDisplayName(displayName: String?) {
    Settings.displayName.value = displayName
  }

  override fun setAvatar(avatar: String?) {
    Settings.avatar.value = avatar
  }

  override fun setSignedIn(signedIn: Boolean) {
    Settings.hasSignedIn.value = signedIn
  }

  override fun setSignInRequired(required: Boolean) {
    Settings.needSignIn.value = required
  }

  override fun setSite(site: SessionGallerySite) {
    Settings.gallerySite.value =
      when (site) {
        SessionGallerySite.PUBLIC -> EhUrl.SITE_E
        SessionGallerySite.EX_HENTAI -> EhUrl.SITE_EX
      }
  }
}

private fun Int.toSessionGallerySite() =
  if (this == EhUrl.SITE_EX) SessionGallerySite.EX_HENTAI else SessionGallerySite.PUBLIC
