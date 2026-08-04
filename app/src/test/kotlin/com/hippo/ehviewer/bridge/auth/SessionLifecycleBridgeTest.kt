package com.hippo.ehviewer.bridge.auth

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class SessionLifecycleBridgeTest {
  @Test
  fun encodesTheRustGeneratedRequestGoldenFixture() {
    val encoded = rustCbor.encodeToByteArray(goldenRequest())

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encoded.toHex())
    assertEquals(goldenRequest(), rustCbor.decodeFromByteArray(encoded))
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedResponseGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<SessionLifecyclePlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )

    val plan = envelope.toSessionLifecyclePlan(goldenSnapshot(), goldenRequest().command)

    assertEquals(SessionLifecycleMode.AUTHENTICATED, plan.state.mode)
    assertEquals("canonical", plan.state.username)
    assertEquals(SessionGallerySite.EX_HENTAI, plan.site)
    assertEquals(SessionProfileUpdate.set("canonical"), plan.profileUpdate)
    assertFalse(plan.clearCookies)
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithOnlySanitizedFacts() {
    val snapshot = goldenSnapshot()
    val command = SessionLifecycleCommand.signInSucceeded("canonical", SessionGallerySite.EX_HENTAI)
    val expected = authenticatedPlan("canonical", SessionGallerySite.EX_HENTAI)
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(version = 1, module = "sessionLifecycle", payload = expected)
      )
    var invocationCount = 0

    val actual =
      planSessionLifecycle(snapshot, command) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        assertEquals(
          SessionLifecycleRequest(version = 1, snapshot = snapshot, command = command),
          rustCbor.decodeFromByteArray(requestBytes),
        )
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(expected, actual)
    assertFailsWith<IllegalArgumentException> {
      planSessionLifecycle(snapshot, command) { _, _, _ -> 0 }
    }
    assertFailsWith<IllegalArgumentException> {
      planSessionLifecycle(snapshot, command) { _, _, output -> output.capacity() + 1 }
    }
  }

  @Test
  fun validatesEveryRestoreStateCombination() {
    assertRestore(
      goldenSnapshot().copy(signInRequired = true),
      nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = true),
    )
    assertRestore(
      goldenSnapshot().copy(storedSignedIn = true, signInRequired = false),
      SessionLifecyclePlan(
        state = SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, "reader"),
        site = SessionGallerySite.EX_HENTAI,
        clearCookies = false,
        profileUpdate = SessionProfileUpdate.keep(),
      ),
    )
    assertRestore(
      goldenSnapshot()
        .copy(
          hasAuthCookies = false,
          storedSignedIn = true,
          signInRequired = false,
        ),
      nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = false),
    )
    assertRestore(
      goldenSnapshot()
        .copy(
          hasAuthCookies = true,
          storedSignedIn = false,
          signInRequired = false,
        ),
      nonAuthenticatedPlan(SessionLifecycleMode.GUEST, clearCookies = true),
    )
    assertRestore(
      goldenSnapshot()
        .copy(
          hasAuthCookies = false,
          storedSignedIn = false,
          signInRequired = false,
        ),
      nonAuthenticatedPlan(SessionLifecycleMode.GUEST, clearCookies = false),
    )
  }

  @Test
  fun validatesSignInGuestAndSignOutPlans() {
    val snapshot = goldenSnapshot()
    val unknownIdentity =
      SessionLifecyclePlan(
        state = SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, "reader"),
        site = SessionGallerySite.PUBLIC,
        clearCookies = false,
        profileUpdate = SessionProfileUpdate.keep(),
      )
    assertEquals(
      unknownIdentity,
      successEnvelope(unknownIdentity)
        .toSessionLifecyclePlan(
          snapshot,
          SessionLifecycleCommand.signInSucceeded(null, SessionGallerySite.PUBLIC),
        ),
    )

    val guest = nonAuthenticatedPlan(SessionLifecycleMode.GUEST, clearCookies = true)
    assertEquals(
      guest,
      successEnvelope(guest).toSessionLifecyclePlan(snapshot, SessionLifecycleCommand.enterGuest()),
    )
    val signedOut = nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = true)
    assertEquals(
      signedOut,
      successEnvelope(signedOut)
        .toSessionLifecyclePlan(snapshot, SessionLifecycleCommand.signOut()),
    )
  }

  @Test
  fun rejectsTypedFailureUnsupportedEnvelopeAndForgedPlan() {
    val snapshot = goldenSnapshot()
    val command = SessionLifecycleCommand.restore()
    val expected = nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, true)

    val failure = RustError(RustErrorKind.NEED_LOGIN)
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<SessionLifecyclePlan>(
            version = 1,
            module = "sessionLifecycle",
            error = failure,
          )
          .toSessionLifecyclePlan(snapshot, command)
      }
    assertEquals(failure, error.failure)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "sessionLifecycle", payload = expected)
        .toSessionLifecyclePlan(snapshot, command)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "sessionPolicy", payload = expected)
        .toSessionLifecyclePlan(snapshot, command)
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(nonAuthenticatedPlan(SessionLifecycleMode.GUEST, true))
        .toSessionLifecyclePlan(snapshot, command)
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(authenticatedPlan("reader", SessionGallerySite.PUBLIC))
        .toSessionLifecyclePlan(
          snapshot.copy(hasAuthCookies = false),
          SessionLifecycleCommand.signInSucceeded("reader", SessionGallerySite.PUBLIC),
        )
    }
  }

  @Test
  fun taggedUnionConstructorsRejectInvalidShapes() {
    assertFailsWith<IllegalArgumentException> {
      SessionLifecycleState(SessionLifecycleMode.GUEST, "reader")
    }
    assertFailsWith<IllegalArgumentException> {
      SessionLifecyclePlan(
        state = SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, "reader"),
        site = SessionGallerySite.PUBLIC,
        clearCookies = true,
        profileUpdate = SessionProfileUpdate.keep(),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      SessionLifecyclePlan(
        state = SessionLifecycleState(SessionLifecycleMode.GUEST),
        site = SessionGallerySite.EX_HENTAI,
        clearCookies = true,
        profileUpdate = SessionProfileUpdate.clear(),
      )
    }

    assertNull(SessionProfileUpdate.keep().username)
    assertNull(SessionProfileUpdate.clear().username)
    assertEquals("reader", SessionProfileUpdate.set("reader").username)
    assertTrue(SessionLifecycleCommand.restore().site == null)
  }

  private fun assertRestore(
    snapshot: SessionLifecycleSnapshot,
    plan: SessionLifecyclePlan,
  ) {
    assertEquals(
      plan,
      successEnvelope(plan).toSessionLifecyclePlan(snapshot, SessionLifecycleCommand.restore()),
    )
  }

  private fun goldenRequest() =
    SessionLifecycleRequest(
      version = 1,
      snapshot = goldenSnapshot(),
      command =
        SessionLifecycleCommand.signInSucceeded(
          "canonical",
          SessionGallerySite.EX_HENTAI,
        ),
    )

  private fun goldenSnapshot() =
    SessionLifecycleSnapshot(
      hasAuthCookies = true,
      storedSignedIn = false,
      signInRequired = true,
      username = "reader",
      site = SessionGallerySite.EX_HENTAI,
    )

  private fun authenticatedPlan(username: String, site: SessionGallerySite) =
    SessionLifecyclePlan(
      state = SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, username),
      site = site,
      clearCookies = false,
      profileUpdate = SessionProfileUpdate.set(username),
    )

  private fun nonAuthenticatedPlan(mode: SessionLifecycleMode, clearCookies: Boolean) =
    SessionLifecyclePlan(
      state = SessionLifecycleState(mode),
      site = SessionGallerySite.PUBLIC,
      clearCookies = clearCookies,
      profileUpdate = SessionProfileUpdate.clear(),
    )

  private fun successEnvelope(plan: SessionLifecyclePlan) =
    ApiEnvelope(version = 1, module = "sessionLifecycle", payload = plan)

  private fun ByteArray.toHex() =
    joinToString(separator = "") { it.toUByte().toString(16).padStart(2, '0') }

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a36776657273696f6e0168736e617073686f74a56e68617341757468436f6f6b696573f56e" +
        "73746f7265645369676e6564496ef46e7369676e496e5265717569726564f568757365726e61" +
        "6d6566726561646572647369746568657848656e74616967636f6d6d616e64a3646b696e646f" +
        "7369676e496e53756363656564656468757365726e616d656963616e6f6e6963616c64736974" +
        "6568657848656e746169"

    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657073657373696f6e4c6966656379636c65677061" +
        "796c6f6164a4657374617465a2646d6f64656d61757468656e7469636174656468757365726e" +
        "616d656963616e6f6e6963616c647369746568657848656e7461696c636c656172436f6f6b69" +
        "6573f46d70726f66696c65557064617465a2646b696e646373657468757365726e616d656963" +
        "616e6f6e6963616c656572726f72f6"
  }
}
