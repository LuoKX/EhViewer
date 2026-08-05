package com.hippo.ehviewer.client.session

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.auth.SessionGallerySite
import com.hippo.ehviewer.bridge.auth.SessionLifecycleCommand
import com.hippo.ehviewer.bridge.auth.SessionLifecycleCommandKind
import com.hippo.ehviewer.bridge.auth.SessionLifecycleMode
import com.hippo.ehviewer.bridge.auth.SessionLifecyclePlan
import com.hippo.ehviewer.bridge.auth.SessionLifecycleSnapshot
import com.hippo.ehviewer.bridge.auth.SessionLifecycleState
import com.hippo.ehviewer.bridge.auth.SessionProfileUpdate
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking

class SessionControllerTest {
  @Test
  fun rustRestoreAndAuthenticationCommitCanonicalState() = runBlocking {
    val store =
      FakeSessionStore(
        hasAuthCookies = true,
        storedSignedIn = false,
        signInRequired = true,
        displayName = "stale",
        avatar = "stale-avatar",
        site = SessionGallerySite.EX_HENTAI,
      )
    val controller = rustController(store)

    controller.restore()

    assertEquals(1, store.clearCount)
    assertFalse(store.storedSignedIn)
    assertTrue(store.signInRequired)
    assertNull(store.displayName)
    assertNull(store.avatar)
    assertEquals(SessionGallerySite.PUBLIC, store.site)

    val attempt = controller.beginSignIn()
    val committed =
      controller.finishSignIn(attempt) {
        store.hasAuthCookies = true
        SessionAuthentication(
          username = "identity",
          profile = SessionProfile("profile", "avatar"),
          site = SessionGallerySite.EX_HENTAI,
        )
      }

    assertTrue(committed)
    assertTrue(store.storedSignedIn)
    assertFalse(store.signInRequired)
    assertEquals("profile", store.displayName)
    assertEquals("avatar", store.avatar)
    assertEquals(SessionGallerySite.EX_HENTAI, store.site)
  }

  @Test
  fun guestAndSignOutApplyDistinctCanonicalModes() = runBlocking {
    val store = authenticatedStore()
    val controller = rustController(store)

    controller.enterGuest()

    assertFalse(store.storedSignedIn)
    assertFalse(store.signInRequired)
    assertNull(store.displayName)
    assertNull(store.avatar)
    assertEquals(SessionGallerySite.PUBLIC, store.site)

    store.hasAuthCookies = true
    store.setSignedIn(true)
    store.setDisplayName("reader")
    store.setAvatar("avatar")
    controller.signOut()

    assertFalse(store.storedSignedIn)
    assertTrue(store.signInRequired)
    assertNull(store.displayName)
    assertNull(store.avatar)
    assertEquals(2, store.clearCount)
  }

  @Test
  fun logoutSupersedesAuthenticationWhileFollowUpDataLoads() = runBlocking {
    val store = FakeSessionStore()
    val controller = SessionController(store, planner = ::canonicalPlan)
    val attempt = controller.beginSignIn()
    store.hasAuthCookies = true
    val started = CompletableDeferred<Unit>()
    val release = CompletableDeferred<Unit>()
    val completion = async {
      controller.finishSignIn(attempt) {
        started.complete(Unit)
        release.await()
        SessionAuthentication("old", null, SessionGallerySite.PUBLIC)
      }
    }

    started.await()
    controller.signOut()
    release.complete(Unit)

    assertFalse(completion.await())
    assertFalse(store.storedSignedIn)
    assertTrue(store.signInRequired)
  }

  @Test
  fun failedTransitionStillInvalidatesTheOlderAttempt() = runBlocking {
    val store = FakeSessionStore()
    var signOutCalls = 0
    val failure = RustFailureException(RustError(RustErrorKind.NEED_LOGIN))
    val controller =
      SessionController(
        store = store,
        planner = { snapshot, command ->
          if (command.kind == SessionLifecycleCommandKind.SIGN_OUT && ++signOutCalls == 2) {
            throw failure
          }
          canonicalPlan(snapshot, command)
        },
      )
    val attempt = controller.beginSignIn()
    store.hasAuthCookies = true
    val started = CompletableDeferred<Unit>()
    val release = CompletableDeferred<Unit>()
    val completion = async {
      controller.finishSignIn(attempt) {
        started.complete(Unit)
        release.await()
        SessionAuthentication("old", null, SessionGallerySite.PUBLIC)
      }
    }

    started.await()
    val thrown = runCatching { controller.signOut() }.exceptionOrNull()
    release.complete(Unit)

    assertEquals(failure, thrown)
    assertFalse(completion.await())
  }

  @Test
  fun failedAuthenticationPlanConsumesItsOneShotAttempt() = runBlocking {
    val store = FakeSessionStore()
    val controller = rustController(store)
    val attempt = controller.beginSignIn()
    var loadCount = 0
    val authentication: suspend () -> SessionAuthentication = {
      loadCount++
      SessionAuthentication("reader", null, SessionGallerySite.PUBLIC)
    }

    val failure = runCatching { controller.finishSignIn(attempt, authentication) }.exceptionOrNull()
    val replayed = controller.finishSignIn(attempt, authentication)

    assertTrue(failure is RustFailureException)
    assertEquals(RustErrorKind.NEED_LOGIN, failure.failure.kind)
    assertFalse(replayed)
    assertEquals(1, loadCount)
  }

  @Test
  fun newerProfileRefreshWinsOverAnOlderResponse() = runBlocking {
    val store = authenticatedStore()
    val controller = SessionController(store, planner = ::canonicalPlan)
    val firstStarted = CompletableDeferred<Unit>()
    val releaseFirst = CompletableDeferred<Unit>()
    val first = async {
      controller.refreshProfile {
        firstStarted.complete(Unit)
        releaseFirst.await()
        SessionProfile("old", "old-avatar")
      }
    }

    firstStarted.await()
    val secondAccepted = controller.refreshProfile { SessionProfile("new", "new-avatar") }
    assertTrue(secondAccepted)
    releaseFirst.complete(Unit)

    assertFalse(first.await())
    assertEquals("new", store.displayName)
    assertEquals("new-avatar", store.avatar)
    store.setSignedIn(false)
    val signedOutAccepted = controller.refreshProfile {
      error("A signed-out profile must not load")
    }
    assertFalse(signedOutAccepted)
  }

  @Test
  fun transportAndLinkageFailuresPropagateBeforeAnyStateIsCommitted() = runBlocking {
    val exceptionStore = authenticatedStore()
    val exception = IllegalArgumentException("bad envelope")
    val exceptionController =
      SessionController(
        store = exceptionStore,
        planner = { _, _ -> throw exception },
      )
    val thrownException = runCatching { exceptionController.signOut() }.exceptionOrNull()

    assertEquals(exception, thrownException)
    assertEquals(0, exceptionStore.clearCount)
    assertTrue(exceptionStore.storedSignedIn)
    assertFalse(exceptionStore.signInRequired)

    val linkageStore = authenticatedStore()
    val linkage = UnsatisfiedLinkError("missing test symbol")
    val linkageController =
      SessionController(
        store = linkageStore,
        planner = { _, _ -> throw linkage },
      )
    val thrownLinkage = runCatching { linkageController.signOut() }.exceptionOrNull()

    assertEquals(linkage, thrownLinkage)
    assertEquals(0, linkageStore.clearCount)
    assertTrue(linkageStore.storedSignedIn)
  }

  @Test
  fun typedRustFailurePropagatesBeforeAuthenticationMutation() = runBlocking {
    val store = authenticatedStore()
    val failure = RustFailureException(RustError(RustErrorKind.NEED_LOGIN))
    val controller =
      SessionController(
        store = store,
        planner = { _, _ -> throw failure },
      )

    val thrown = runCatching { controller.signOut() }.exceptionOrNull()

    assertEquals(failure, thrown)
    assertEquals(0, store.clearCount)
    assertTrue(store.storedSignedIn)
  }

  private fun rustController(store: FakeSessionStore) =
    SessionController(
      store = store,
      planner = ::canonicalPlan,
    )

  private fun authenticatedStore() =
    FakeSessionStore(
      hasAuthCookies = true,
      storedSignedIn = true,
      signInRequired = false,
      displayName = "reader",
      avatar = "avatar",
      site = SessionGallerySite.EX_HENTAI,
    )

  private companion object {
    fun canonicalPlan(
      snapshot: SessionLifecycleSnapshot,
      command: SessionLifecycleCommand,
    ): SessionLifecyclePlan =
      when (command.kind) {
        SessionLifecycleCommandKind.RESTORE ->
          when {
            snapshot.signInRequired ->
              nonAuthenticated(SessionLifecycleMode.SIGN_IN_REQUIRED, snapshot.hasAuthCookies)
            snapshot.storedSignedIn && snapshot.hasAuthCookies ->
              SessionLifecyclePlan(
                state =
                  SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, snapshot.username),
                site = snapshot.site,
                clearCookies = false,
                profileUpdate = SessionProfileUpdate.keep(),
              )
            snapshot.storedSignedIn ->
              nonAuthenticated(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = false)
            else -> nonAuthenticated(SessionLifecycleMode.GUEST, snapshot.hasAuthCookies)
          }
        SessionLifecycleCommandKind.SIGN_IN_SUCCEEDED -> {
          if (!snapshot.hasAuthCookies) {
            throw RustFailureException(RustError(RustErrorKind.NEED_LOGIN))
          }
          val username = command.username
          SessionLifecyclePlan(
            state =
              SessionLifecycleState(
                SessionLifecycleMode.AUTHENTICATED,
                username ?: snapshot.username,
              ),
            site = requireNotNull(command.site),
            clearCookies = false,
            profileUpdate = username?.let(SessionProfileUpdate::set) ?: SessionProfileUpdate.keep(),
          )
        }
        SessionLifecycleCommandKind.ENTER_GUEST ->
          nonAuthenticated(SessionLifecycleMode.GUEST, clearCookies = true)
        SessionLifecycleCommandKind.SIGN_OUT ->
          nonAuthenticated(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = true)
      }

    private fun nonAuthenticated(mode: SessionLifecycleMode, clearCookies: Boolean) =
      SessionLifecyclePlan(
        state = SessionLifecycleState(mode),
        site = SessionGallerySite.PUBLIC,
        clearCookies = clearCookies,
        profileUpdate = SessionProfileUpdate.clear(),
      )
  }
}

private class FakeSessionStore(
  var hasAuthCookies: Boolean = false,
  storedSignedIn: Boolean = false,
  signInRequired: Boolean = true,
  displayName: String? = null,
  avatar: String? = null,
  site: SessionGallerySite = SessionGallerySite.PUBLIC,
) : SessionStore {
  var clearCount = 0
  private var currentSignedIn = storedSignedIn
  private var currentSignInRequired = signInRequired
  private var currentDisplayName = displayName
  private var currentAvatar = avatar
  private var currentSite = site

  val storedSignedIn: Boolean
    get() = currentSignedIn

  val signInRequired: Boolean
    get() = currentSignInRequired

  val displayName: String?
    get() = currentDisplayName

  val avatar: String?
    get() = currentAvatar

  val site: SessionGallerySite
    get() = currentSite

  override fun snapshot() =
    SessionLifecycleSnapshot(
      hasAuthCookies = hasAuthCookies,
      storedSignedIn = storedSignedIn,
      signInRequired = signInRequired,
      username = displayName,
      site = site,
    )

  override suspend fun clearCookies() {
    clearCount++
    hasAuthCookies = false
  }

  override fun setDisplayName(displayName: String?) {
    currentDisplayName = displayName
  }

  override fun setAvatar(avatar: String?) {
    currentAvatar = avatar
  }

  override fun setSignedIn(signedIn: Boolean) {
    currentSignedIn = signedIn
  }

  override fun setSignInRequired(required: Boolean) {
    currentSignInRequired = required
  }

  override fun setSite(site: SessionGallerySite) {
    currentSite = site
  }
}
