package com.hippo.ehviewer.bridge.auth

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Device contract tests for the real session-lifecycle JNI and CBOR boundary. */
@RunWith(AndroidJUnit4::class)
class SessionLifecycleDeviceTest {
  @Test
  fun restoresAndCompletesAuthenticationThroughTheNativePlanner() {
    val restored =
      planSessionLifecycle(
        snapshot = signedInSnapshot(),
        command = SessionLifecycleCommand.restore(),
      )

    assertEquals(SessionLifecycleMode.AUTHENTICATED, restored.state.mode)
    assertEquals("reader", restored.state.username)
    assertEquals(SessionGallerySite.EX_HENTAI, restored.site)
    assertEquals(SessionProfileUpdateKind.KEEP, restored.profileUpdate.kind)
    assertFalse(restored.clearCookies)

    val authenticated =
      planSessionLifecycle(
        snapshot = signedInSnapshot().copy(storedSignedIn = false, signInRequired = true),
        command =
          SessionLifecycleCommand.signInSucceeded(
            "canonical",
            SessionGallerySite.PUBLIC,
          ),
      )

    assertEquals(SessionLifecycleMode.AUTHENTICATED, authenticated.state.mode)
    assertEquals("canonical", authenticated.state.username)
    assertEquals(SessionGallerySite.PUBLIC, authenticated.site)
    assertEquals(SessionProfileUpdate.set("canonical"), authenticated.profileUpdate)
  }

  @Test
  fun returnsTypedNeedLoginWhenIdentityCookiesAreMissing() {
    val failure =
      assertThrows(RustFailureException::class.java) {
        planSessionLifecycle(
          snapshot = signedInSnapshot().copy(hasAuthCookies = false),
          command =
            SessionLifecycleCommand.signInSucceeded(
              "reader",
              SessionGallerySite.PUBLIC,
            ),
        )
      }

    assertEquals(RustErrorKind.NEED_LOGIN, failure.failure.kind)
  }

  @Test
  fun plansGuestAndSignOutThroughTheNativePlanner() {
    val guest =
      planSessionLifecycle(
        snapshot = signedInSnapshot(),
        command = SessionLifecycleCommand.enterGuest(),
      )
    val signedOut =
      planSessionLifecycle(
        snapshot = signedInSnapshot(),
        command = SessionLifecycleCommand.signOut(),
      )

    assertEquals(SessionLifecycleMode.GUEST, guest.state.mode)
    assertEquals(SessionLifecycleMode.SIGN_IN_REQUIRED, signedOut.state.mode)
    listOf(guest, signedOut).forEach { plan ->
      assertEquals(SessionGallerySite.PUBLIC, plan.site)
      assertEquals(SessionProfileUpdateKind.CLEAR, plan.profileUpdate.kind)
      assertEquals(true, plan.clearCookies)
    }
  }

  private fun signedInSnapshot() =
    SessionLifecycleSnapshot(
      hasAuthCookies = true,
      storedSignedIn = true,
      signInRequired = false,
      username = "reader",
      site = SessionGallerySite.EX_HENTAI,
    )
}
