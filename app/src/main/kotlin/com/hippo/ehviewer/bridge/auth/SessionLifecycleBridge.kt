package com.hippo.ehviewer.bridge.auth

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Stable navigation and authentication modes owned by `eh_domain::session::lifecycle`. */
@Serializable
internal enum class SessionLifecycleMode {
  @SerialName("signInRequired") SIGN_IN_REQUIRED,
  @SerialName("guest") GUEST,
  @SerialName("authenticated") AUTHENTICATED,
}

/** Canonical account state emitted by the V1 session-lifecycle planner. */
@Serializable
internal data class SessionLifecycleState(
  val mode: SessionLifecycleMode,
  val username: String? = null,
) {
  init {
    require(mode == SessionLifecycleMode.AUTHENTICATED || username == null) {
      "A non-authenticated Session Lifecycle state cannot contain a username"
    }
  }
}

/** Sanitized Android session facts sent to Rust without credentials or cookie values. */
@Serializable
internal data class SessionLifecycleSnapshot(
  val hasAuthCookies: Boolean,
  val storedSignedIn: Boolean,
  val signInRequired: Boolean,
  val username: String? = null,
  val site: SessionGallerySite,
)

/** Stable command discriminants accepted by the V1 session-lifecycle reducer. */
@Serializable
internal enum class SessionLifecycleCommandKind {
  @SerialName("restore") RESTORE,
  @SerialName("signInSucceeded") SIGN_IN_SUCCEEDED,
  @SerialName("enterGuest") ENTER_GUEST,
  @SerialName("signOut") SIGN_OUT,
}

/**
 * One lifecycle event already observed by Android.
 *
 * The nullable fields form a tagged union keyed by [kind]. Factory methods are the supported
 * construction path and keep the Kotlin shape identical to the Rust tagged enum.
 */
@ConsistentCopyVisibility
@Serializable
internal data class SessionLifecycleCommand
private constructor(
  val kind: SessionLifecycleCommandKind,
  val username: String? = null,
  val site: SessionGallerySite? = null,
) {
  init {
    require(
      when (kind) {
        SessionLifecycleCommandKind.SIGN_IN_SUCCEEDED -> site != null
        else -> username == null && site == null
      }
    ) {
      "Session Lifecycle command fields do not match $kind"
    }
  }

  companion object {
    /** Reconciles persisted flags with the current identity-cookie presence at startup. */
    fun restore() = SessionLifecycleCommand(SessionLifecycleCommandKind.RESTORE)

    /** Completes an Android-owned authentication after its identity cookies are observable. */
    fun signInSucceeded(username: String?, site: SessionGallerySite) =
      SessionLifecycleCommand(SessionLifecycleCommandKind.SIGN_IN_SUCCEEDED, username, site)

    /** Selects anonymous access to the public gallery. */
    fun enterGuest() = SessionLifecycleCommand(SessionLifecycleCommandKind.ENTER_GUEST)

    /** Ends the current session and requires a new authentication choice. */
    fun signOut() = SessionLifecycleCommand(SessionLifecycleCommandKind.SIGN_OUT)
  }
}

/** Versioned V1 request consumed by the Rust session-lifecycle reducer. */
@Serializable
internal data class SessionLifecycleRequest(
  val version: Int,
  val snapshot: SessionLifecycleSnapshot,
  val command: SessionLifecycleCommand,
)

/** Stable profile-mutation discriminants emitted by the V1 planner. */
@Serializable
internal enum class SessionProfileUpdateKind {
  @SerialName("keep") KEEP,
  @SerialName("set") SET,
  @SerialName("clear") CLEAR,
}

/** One profile mutation Android applies only after accepting the complete Rust plan. */
@ConsistentCopyVisibility
@Serializable
internal data class SessionProfileUpdate
private constructor(
  val kind: SessionProfileUpdateKind,
  val username: String? = null,
) {
  init {
    require((kind == SessionProfileUpdateKind.SET) == (username != null)) {
      "Session Lifecycle profile-update fields do not match $kind"
    }
  }

  companion object {
    /** Preserves the last accepted profile. */
    fun keep() = SessionProfileUpdate(SessionProfileUpdateKind.KEEP)

    /** Stores a newly confirmed display name. */
    fun set(username: String) = SessionProfileUpdate(SessionProfileUpdateKind.SET, username)

    /** Removes all account presentation data. */
    fun clear() = SessionProfileUpdate(SessionProfileUpdateKind.CLEAR)
  }
}

/** Complete deterministic session transition returned by Rust. */
@Serializable
internal data class SessionLifecyclePlan(
  val state: SessionLifecycleState,
  val site: SessionGallerySite,
  val clearCookies: Boolean,
  val profileUpdate: SessionProfileUpdate,
) {
  init {
    if (state.mode == SessionLifecycleMode.AUTHENTICATED) {
      require(!clearCookies && profileUpdate.kind != SessionProfileUpdateKind.CLEAR) {
        "An authenticated Session Lifecycle plan cannot clear its identity"
      }
    } else {
      require(
        state.username == null &&
          site == SessionGallerySite.PUBLIC &&
          profileUpdate.kind == SessionProfileUpdateKind.CLEAR
      ) {
        "A non-authenticated Session Lifecycle plan must clear its public identity"
      }
    }
  }
}

/** Encodes sanitized session facts, invokes Rust, and validates the exact returned transition. */
internal fun planSessionLifecycle(
  snapshot: SessionLifecycleSnapshot,
  command: SessionLifecycleCommand,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanSessionLifecycleV1,
): SessionLifecyclePlan {
  val request =
    rustCbor.encodeToByteArray(
      SessionLifecycleRequest(SESSION_LIFECYCLE_API_VERSION, snapshot, command)
    )
  return invokeRustCbor<ApiEnvelope<SessionLifecyclePlan>>(
      request,
      operation = "Session Lifecycle",
      nativeCall = nativeCall,
    )
    .toSessionLifecyclePlan(snapshot, command)
}

/** Rejects unsupported envelopes, typed failures, and forged transition plans. */
internal fun ApiEnvelope<SessionLifecyclePlan>.toSessionLifecyclePlan(
  snapshot: SessionLifecycleSnapshot,
  command: SessionLifecycleCommand,
): SessionLifecyclePlan {
  require(version == SESSION_LIFECYCLE_API_VERSION) {
    "Unsupported Session Lifecycle API version: $version"
  }
  require(module == SESSION_LIFECYCLE_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Session Lifecycle response has no payload" }
    .also { plan ->
      require(plan == expectedPlan(snapshot, command)) {
        "Rust Session Lifecycle plan does not match the sanitized session facts"
      }
    }
}

private fun expectedPlan(
  snapshot: SessionLifecycleSnapshot,
  command: SessionLifecycleCommand,
): SessionLifecyclePlan =
  when (command.kind) {
    SessionLifecycleCommandKind.RESTORE ->
      when {
        snapshot.signInRequired ->
          nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, snapshot.hasAuthCookies)
        snapshot.storedSignedIn && snapshot.hasAuthCookies ->
          authenticatedPlan(snapshot.username, snapshot.site, SessionProfileUpdate.keep())
        snapshot.storedSignedIn ->
          nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = false)
        else -> nonAuthenticatedPlan(SessionLifecycleMode.GUEST, snapshot.hasAuthCookies)
      }
    SessionLifecycleCommandKind.SIGN_IN_SUCCEEDED -> {
      require(snapshot.hasAuthCookies) {
        "A successful Session Lifecycle plan requires both identity cookies"
      }
      val username = command.username
      authenticatedPlan(
        username ?: snapshot.username,
        requireNotNull(command.site),
        username?.let(SessionProfileUpdate::set) ?: SessionProfileUpdate.keep(),
      )
    }
    SessionLifecycleCommandKind.ENTER_GUEST ->
      nonAuthenticatedPlan(SessionLifecycleMode.GUEST, clearCookies = true)
    SessionLifecycleCommandKind.SIGN_OUT ->
      nonAuthenticatedPlan(SessionLifecycleMode.SIGN_IN_REQUIRED, clearCookies = true)
  }

private fun authenticatedPlan(
  username: String?,
  site: SessionGallerySite,
  profileUpdate: SessionProfileUpdate,
) =
  SessionLifecyclePlan(
    state = SessionLifecycleState(SessionLifecycleMode.AUTHENTICATED, username),
    site = site,
    clearCookies = false,
    profileUpdate = profileUpdate,
  )

private fun nonAuthenticatedPlan(mode: SessionLifecycleMode, clearCookies: Boolean) =
  SessionLifecyclePlan(
    state = SessionLifecycleState(mode),
    site = SessionGallerySite.PUBLIC,
    clearCookies = clearCookies,
    profileUpdate = SessionProfileUpdate.clear(),
  )

/** Invokes the V1 `eh_domain::session::lifecycle` planner using separate direct buffers. */
private external fun nativePlanSessionLifecycleV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private const val SESSION_LIFECYCLE_API_VERSION = 1
private const val SESSION_LIFECYCLE_MODULE = "sessionLifecycle"
