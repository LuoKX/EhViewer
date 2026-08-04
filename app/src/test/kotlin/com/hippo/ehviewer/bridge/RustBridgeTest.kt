package com.hippo.ehviewer.bridge

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class RustBridgeTest {
  @Test
  fun requestAndResponseCborGoldensMatchRustExactly() {
    val request = RustCoreHealthRequest(version = 1, nonce = 42)
    assertEquals(REQUEST_CBOR_HEX, rustCbor.encodeToByteArray(request).toHex())

    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<RustCoreHealth>>(RESPONSE_CBOR_HEX.decodeHex())
    assertEquals(1, envelope.version)
    assertEquals("rustCore", envelope.module)
    assertEquals(RustCoreHealth(protocolVersion = 1, nonce = 42), envelope.payload)
    assertEquals(null, envelope.error)
  }

  @Test
  fun callbackReceivesTheGoldenRequestAndReturnsTheGoldenResponseExactlyOnce() {
    var calls = 0
    val result =
      RustBridge.ping(nonce = 42) { requestBuffer, requestSize, responseBuffer ->
        calls += 1
        val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
        assertEquals(REQUEST_CBOR_HEX, requestBytes.toHex())
        assertEquals(
          RustCoreHealthRequest(version = 1, nonce = 42),
          rustCbor.decodeFromByteArray(requestBytes),
        )
        val response = RESPONSE_CBOR_HEX.decodeHex()
        responseBuffer.put(response)
        response.size
      }

    assertTrue(result)
    assertEquals(1, calls)
  }

  @Test
  fun allMethodsSucceedWithOneProbeEach() {
    var calls = 0
    val nativeCall = echoingNativeCall { calls += 1 }

    assertTrue(RustBridge.isReady(nativeCall))
    assertTrue(RustBridge.checkVersion(nativeCall = nativeCall))
    assertTrue(RustBridge.ping(Long.MIN_VALUE, nativeCall))
    assertEquals(3, calls)
  }

  @Test
  fun protocolAndNonceMismatchesReturnFalse() {
    assertFalse(
      RustBridge.checkVersion(
        expectedVersion = 2,
        nativeCall = respondingWith(RustCoreHealth(protocolVersion = 1, nonce = probeNonce)),
      )
    )
    assertFalse(
      RustBridge.checkVersion(
        nativeCall = respondingWith(RustCoreHealth(protocolVersion = 1, nonce = probeNonce + 1))
      )
    )
    assertFalse(
      RustBridge.ping(
        nonce = 42,
        nativeCall = respondingWith(RustCoreHealth(protocolVersion = 1, nonce = 43)),
      )
    )
    assertFalse(
      RustBridge.isReady(respondingWith(RustCoreHealth(protocolVersion = 2, nonce = probeNonce)))
    )
  }

  @Test
  fun strictProbeRejectsWrongMetadataTypedFailureAndMissingOrInvalidPayload() {
    assertFailsWith<IllegalArgumentException> {
      RustBridge.checkVersion(nativeCall = respondingWith(version = 2))
    }
    assertFailsWith<IllegalArgumentException> {
      RustBridge.checkVersion(nativeCall = respondingWith(module = "other"))
    }
    assertFailsWith<RustFailureException> {
      RustBridge.checkVersion(
        nativeCall =
          respondingWith(
            error = RustError(RustErrorKind.ERROR, "fixed health failure"),
            payload = null,
          )
      )
    }
    assertFailsWith<IllegalArgumentException> {
      RustBridge.checkVersion(nativeCall = returning(missingPayloadBytes()))
    }
    assertFailsWith<Exception> {
      RustBridge.checkVersion(nativeCall = returning(invalidPayloadBytes()))
    }
  }

  @Test
  fun malformedCborInvalidSizeAndNativeExceptionAreBoundaryFailures() {
    assertFailsWith<Exception> {
      RustBridge.checkVersion(nativeCall = returning(byteArrayOf(0xff.toByte())))
    }
    assertFailsWith<IllegalArgumentException> {
      RustBridge.checkVersion { _, _, _ -> 0 }
    }
    assertFailsWith<IllegalArgumentException> {
      RustBridge.checkVersion { _, _, _ -> Int.MAX_VALUE }
    }
    assertFailsWith<IllegalStateException> {
      RustBridge.checkVersion { _, _, _ -> throw IllegalStateException("native failure") }
    }
  }

  @Test
  fun readinessContainsRecoverableExceptionsAndLinkageErrors() {
    assertFalse(RustBridge.isReady { _, _, _ -> throw IllegalStateException("recoverable") })
    assertFalse(RustBridge.isReady { _, _, _ -> throw LinkageError("missing JNI") })
  }

  @Test
  fun fatalErrorsPropagateFromEveryMethod() {
    assertFailsWith<AssertionError> {
      RustBridge.isReady { _, _, _ -> throw AssertionError("fatal") }
    }
    assertFailsWith<AssertionError> {
      RustBridge.checkVersion { _, _, _ -> throw AssertionError("fatal") }
    }
    assertFailsWith<AssertionError> {
      RustBridge.ping(nativeCall = { _, _, _ -> throw AssertionError("fatal") })
    }
  }

  @Test
  fun strictMethodsPropagateTransportAndLinkageFailures() {
    assertFailsWith<IllegalStateException> {
      RustBridge.checkVersion { _, _, _ -> throw IllegalStateException("transport") }
    }
    assertFailsWith<LinkageError> {
      RustBridge.checkVersion { _, _, _ -> throw LinkageError("missing JNI") }
    }
    assertFailsWith<IllegalStateException> {
      RustBridge.ping(nativeCall = { _, _, _ -> throw IllegalStateException("transport") })
    }
    assertFailsWith<LinkageError> {
      RustBridge.ping(nativeCall = { _, _, _ -> throw LinkageError("missing JNI") })
    }
  }

  private fun echoingNativeCall(onCall: () -> Unit = {}): RustCoreHealthNativeCall =
    { requestBuffer, requestSize, responseBuffer ->
      onCall()
      val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
      val request = rustCbor.decodeFromByteArray<RustCoreHealthRequest>(requestBytes)
      val response =
        ApiEnvelope(
          version = 1,
          module = "rustCore",
          payload = RustCoreHealth(protocolVersion = 1, nonce = request.nonce),
        )
      val responseBytes = rustCbor.encodeToByteArray(response)
      responseBuffer.put(responseBytes)
      responseBytes.size
    }

  private fun respondingWith(
    payload: RustCoreHealth? = RustCoreHealth(protocolVersion = 1, nonce = probeNonce),
    version: Int = 1,
    module: String = "rustCore",
    error: RustError? = null,
  ): RustCoreHealthNativeCall =
    returning(rustCbor.encodeToByteArray(WireEnvelope(version, module, payload, error)))

  private fun returning(response: ByteArray): RustCoreHealthNativeCall = { _, _, output ->
    output.put(response)
    response.size
  }

  private fun missingPayloadBytes() =
    rustCbor.encodeToByteArray(
      WireEnvelope<RustCoreHealth>(version = 1, module = "rustCore", payload = null, error = null)
    )

  private fun invalidPayloadBytes() =
    rustCbor.encodeToByteArray(
      WireEnvelope(
        version = 1,
        module = "rustCore",
        payload = InvalidHealthPayload(protocolVersion = "one", nonce = 42),
        error = null,
      )
    )

  private fun ByteArray.toHex() =
    joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

  @Serializable
  private data class WireEnvelope<T>(
    val version: Int,
    val module: String,
    val payload: T? = null,
    val error: RustError? = null,
  )

  @Serializable
  private data class InvalidHealthPayload(val protocolVersion: String, val nonce: Long)

  private companion object {
    const val REQUEST_CBOR_HEX = "a26776657273696f6e01656e6f6e6365182a"
    const val RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656872757374436f7265677061796c6f6164a26f70726f746f636f6c56657273696f6e01656e6f6e6365182a656572726f72f6"
    const val probeNonce = RUST_CORE_HEALTH_PROBE_NONCE
  }
}
