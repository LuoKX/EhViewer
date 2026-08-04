package com.hippo.ehviewer.bridge.auth

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Sanitized host category used only by the empty-response IP-ban policy. */
@Serializable
internal enum class SessionGallerySite {
  @SerialName("public") PUBLIC,
  @SerialName("exHentai") EX_HENTAI,
}

/** Stable source discriminants accepted by the V1 session-failure policy. */
@Serializable
internal enum class SessionFailureSourceKind {
  @SerialName("parse") PARSE,
  @SerialName("other") OTHER,
}

/** Sanitized category of the original Android exception. */
@ConsistentCopyVisibility
@Serializable
internal data class SessionFailureSource
private constructor(
  val kind: SessionFailureSourceKind,
  val nonHtmlText: Boolean? = null,
) {
  init {
    require(
      when (kind) {
        SessionFailureSourceKind.PARSE -> nonHtmlText != null
        SessionFailureSourceKind.OTHER -> nonHtmlText == null
      }
    ) {
      "Session Policy failure-source fields do not match $kind"
    }
  }

  companion object {
    /** Marks a parser/serializer failure without exposing its response body. */
    fun parse(nonHtmlText: Boolean) =
      SessionFailureSource(SessionFailureSourceKind.PARSE, nonHtmlText)

    /** Marks any non-parser throwable which Android may need to rethrow unchanged. */
    fun other() = SessionFailureSource(SessionFailureSourceKind.OTHER)
  }
}

/** Sanitized response facts sent to `eh_domain::session`. */
@Serializable
internal data class SessionFailureSnapshot(
  val site: SessionGallerySite,
  val cloudflareChallenge: Boolean,
  val emptyBody: Boolean,
  val domainError: RustError? = null,
  val httpStatusSuccess: Boolean,
  val source: SessionFailureSource,
)

/** Versioned V1 request consumed by the Rust session-failure policy. */
@Serializable
internal data class SessionFailureRequest(
  val version: Int,
  val snapshot: SessionFailureSnapshot,
)

/** Stable decision discriminants emitted by the V1 session-failure policy. */
@Serializable
internal enum class SessionFailureDecisionKind {
  @SerialName("cloudflareChallenge") CLOUDFLARE_CHALLENGE,
  @SerialName("emptyBodyIpBanned") EMPTY_BODY_IP_BANNED,
  @SerialName("domain") DOMAIN,
  @SerialName("httpStatus") HTTP_STATUS,
  @SerialName("nonHtmlTextIpBanned") NON_HTML_TEXT_IP_BANNED,
  @SerialName("parseFailure") PARSE_FAILURE,
  @SerialName("rethrow") RETHROW,
}

/** One platform-neutral failure action selected by Rust. */
@Serializable
internal data class SessionFailureDecision(
  val kind: SessionFailureDecisionKind,
  val site: SessionGallerySite? = null,
  val error: RustError? = null,
) {
  init {
    require(
      when (kind) {
        SessionFailureDecisionKind.EMPTY_BODY_IP_BANNED -> site != null && error == null
        SessionFailureDecisionKind.DOMAIN -> site == null && error != null
        else -> site == null && error == null
      }
    ) {
      "Session Policy decision fields do not match $kind"
    }
  }
}

/** Deterministic V1 failure decision returned by Rust. */
@Serializable internal data class SessionFailurePlan(val decision: SessionFailureDecision)

/** Encodes sanitized response facts, invokes Rust, and validates the exact decision. */
internal fun classifySessionFailure(
  snapshot: SessionFailureSnapshot,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeClassifySessionFailureV1,
): SessionFailurePlan {
  val request =
    rustCbor.encodeToByteArray(SessionFailureRequest(SESSION_POLICY_API_VERSION, snapshot))
  return invokeRustCbor<ApiEnvelope<SessionFailurePlan>>(
      request,
      operation = "Session Policy",
      nativeCall = nativeCall,
    )
    .toSessionFailurePlan(snapshot)
}

/** Rejects unsupported envelopes or a decision inconsistent with the sanitized request. */
internal fun ApiEnvelope<SessionFailurePlan>.toSessionFailurePlan(
  snapshot: SessionFailureSnapshot
): SessionFailurePlan {
  require(version == SESSION_POLICY_API_VERSION) {
    "Unsupported Session Policy API version: $version"
  }
  require(module == SESSION_POLICY_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Session Policy response has no payload" }
    .also { plan ->
      require(plan.decision == expectedDecision(snapshot)) {
        "Rust Session Policy decision does not match the sanitized response facts"
      }
    }
}

private fun expectedDecision(snapshot: SessionFailureSnapshot): SessionFailureDecision =
  when {
    snapshot.cloudflareChallenge ->
      SessionFailureDecision(SessionFailureDecisionKind.CLOUDFLARE_CHALLENGE)
    snapshot.emptyBody ->
      SessionFailureDecision(
        kind = SessionFailureDecisionKind.EMPTY_BODY_IP_BANNED,
        site = snapshot.site,
      )
    snapshot.domainError?.kind == RustErrorKind.GALLERY_UNAVAILABLE ->
      SessionFailureDecision(
        kind = SessionFailureDecisionKind.DOMAIN,
        error = snapshot.domainError,
      )
    !snapshot.httpStatusSuccess -> SessionFailureDecision(SessionFailureDecisionKind.HTTP_STATUS)
    snapshot.domainError != null ->
      SessionFailureDecision(
        kind = SessionFailureDecisionKind.DOMAIN,
        error = snapshot.domainError,
      )
    snapshot.source.kind == SessionFailureSourceKind.PARSE && snapshot.source.nonHtmlText == true ->
      SessionFailureDecision(SessionFailureDecisionKind.NON_HTML_TEXT_IP_BANNED)
    snapshot.source.kind == SessionFailureSourceKind.PARSE ->
      SessionFailureDecision(SessionFailureDecisionKind.PARSE_FAILURE)
    else -> SessionFailureDecision(SessionFailureDecisionKind.RETHROW)
  }

/** Invokes the V1 `eh_domain::session` policy using separate direct buffers. */
private external fun nativeClassifySessionFailureV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private const val SESSION_POLICY_API_VERSION = 1
private const val SESSION_POLICY_MODULE = "sessionPolicy"
