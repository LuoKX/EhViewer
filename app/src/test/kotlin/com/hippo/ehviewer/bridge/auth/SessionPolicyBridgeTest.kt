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
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class SessionPolicyBridgeTest {
  @Test
  fun encodesTheRustGeneratedRequestGoldenFixture() {
    val request = SessionFailureRequest(version = 1, snapshot = goldenSnapshot())

    val encodedHex =
      rustCbor.encodeToByteArray(request).joinToString(separator = "") {
        it.toUByte().toString(16).padStart(2, '0')
      }

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encodedHex)
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedResponseGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<SessionFailurePlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )

    val plan = envelope.toSessionFailurePlan(goldenSnapshot())

    assertEquals(SessionFailureDecisionKind.DOMAIN, plan.decision.kind)
    assertEquals(RustError(RustErrorKind.NEED_LOGIN), plan.decision.error)
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithOnlySanitizedFacts() {
    val snapshot = goldenSnapshot()
    val expected =
      SessionFailurePlan(
        SessionFailureDecision(
          kind = SessionFailureDecisionKind.DOMAIN,
          error = RustError(RustErrorKind.NEED_LOGIN),
        )
      )
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(version = 1, module = "sessionPolicy", payload = expected)
      )
    var invocationCount = 0

    val actual =
      classifySessionFailure(snapshot) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        val decoded = rustCbor.decodeFromByteArray<SessionFailureRequest>(requestBytes)
        assertEquals(snapshot, decoded.snapshot)
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(expected, actual)
    assertFailsWith<IllegalArgumentException> {
      classifySessionFailure(snapshot) { _, _, _ -> 0 }
    }
    assertFailsWith<IllegalArgumentException> {
      classifySessionFailure(snapshot) { _, _, output -> output.capacity() + 1 }
    }
  }

  @Test
  fun validatesTheCompleteLegacyFailurePrecedence() {
    val challenge = baseSnapshot().copy(cloudflareChallenge = true, emptyBody = true)
    assertDecision(
      challenge,
      SessionFailureDecision(SessionFailureDecisionKind.CLOUDFLARE_CHALLENGE),
    )

    val empty = baseSnapshot().copy(site = SessionGallerySite.EX_HENTAI, emptyBody = true)
    assertDecision(
      empty,
      SessionFailureDecision(
        kind = SessionFailureDecisionKind.EMPTY_BODY_IP_BANNED,
        site = SessionGallerySite.EX_HENTAI,
      ),
    )

    val unavailable =
      baseSnapshot()
        .copy(
          domainError = RustError(RustErrorKind.GALLERY_UNAVAILABLE, "removed"),
          httpStatusSuccess = false,
        )
    assertDecision(
      unavailable,
      SessionFailureDecision(
        kind = SessionFailureDecisionKind.DOMAIN,
        error = unavailable.domainError,
      ),
    )

    val statusBeforeDomain =
      baseSnapshot()
        .copy(
          domainError = RustError(RustErrorKind.NEED_LOGIN),
          httpStatusSuccess = false,
        )
    assertDecision(
      statusBeforeDomain,
      SessionFailureDecision(SessionFailureDecisionKind.HTTP_STATUS),
    )

    assertDecision(
      baseSnapshot().copy(source = SessionFailureSource.parse(nonHtmlText = true)),
      SessionFailureDecision(SessionFailureDecisionKind.NON_HTML_TEXT_IP_BANNED),
    )
    assertDecision(
      baseSnapshot().copy(source = SessionFailureSource.parse(nonHtmlText = false)),
      SessionFailureDecision(SessionFailureDecisionKind.PARSE_FAILURE),
    )
    assertDecision(baseSnapshot(), SessionFailureDecision(SessionFailureDecisionKind.RETHROW))
  }

  @Test
  fun rejectsWrongEnvelopeTypedFailureAndForgedDecisions() {
    val snapshot = baseSnapshot()
    val expected = SessionFailurePlan(SessionFailureDecision(SessionFailureDecisionKind.RETHROW))

    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "sessionPolicy", payload = expected)
        .toSessionFailurePlan(snapshot)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "auth", payload = expected).toSessionFailurePlan(snapshot)
    }

    val failure = RustError(RustErrorKind.ERROR, "invalid session facts")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<SessionFailurePlan>(version = 1, module = "sessionPolicy", error = failure)
          .toSessionFailurePlan(snapshot)
      }
    assertEquals(failure, error.failure)

    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          version = 1,
          module = "sessionPolicy",
          payload =
            SessionFailurePlan(
              SessionFailureDecision(SessionFailureDecisionKind.CLOUDFLARE_CHALLENGE)
            ),
        )
        .toSessionFailurePlan(snapshot)
    }
    assertFailsWith<IllegalArgumentException> {
      SessionFailureDecision(SessionFailureDecisionKind.EMPTY_BODY_IP_BANNED)
    }
    assertFailsWith<IllegalArgumentException> {
      SessionFailureDecision(SessionFailureDecisionKind.DOMAIN)
    }
  }

  @Test
  fun failureSourceFactoriesPreserveTheirTaggedUnionShape() {
    assertEquals(true, SessionFailureSource.parse(nonHtmlText = true).nonHtmlText)
    assertEquals(SessionFailureSourceKind.OTHER, SessionFailureSource.other().kind)
  }

  private fun assertDecision(
    snapshot: SessionFailureSnapshot,
    decision: SessionFailureDecision,
  ) {
    val plan = SessionFailurePlan(decision)
    assertEquals(
      plan,
      ApiEnvelope(version = 1, module = "sessionPolicy", payload = plan)
        .toSessionFailurePlan(snapshot),
    )
  }

  private fun goldenSnapshot() =
    SessionFailureSnapshot(
      site = SessionGallerySite.EX_HENTAI,
      cloudflareChallenge = false,
      emptyBody = false,
      domainError = RustError(RustErrorKind.NEED_LOGIN),
      httpStatusSuccess = true,
      source = SessionFailureSource.parse(nonHtmlText = false),
    )

  private fun baseSnapshot() =
    SessionFailureSnapshot(
      site = SessionGallerySite.PUBLIC,
      cloudflareChallenge = false,
      emptyBody = false,
      httpStatusSuccess = true,
      source = SessionFailureSource.other(),
    )

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a26776657273696f6e0168736e617073686f74a6647369746568657848656e74616973636c6f" +
        "7564666c6172654368616c6c656e6765f469656d707479426f6479f46b646f6d61696e457272" +
        "6f72a1646b696e64696e6565644c6f67696e716874747053746174757353756363657373f566" +
        "736f75726365a2646b696e646570617273656b6e6f6e48746d6c54657874f4"

    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d73657373696f6e506f6c696379677061796c6f" +
        "6164a1686465636973696f6ea2646b696e6466646f6d61696e656572726f72a1646b696e6469" +
        "6e6565644c6f67696e656572726f72f6"
  }
}
