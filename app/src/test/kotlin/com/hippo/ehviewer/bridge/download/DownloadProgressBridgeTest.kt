package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class DownloadProgressBridgeTest {
  @Test
  fun requestAndResponseCborGoldensMatchRustExactly() {
    assertEquals(REQUEST_CBOR_HEX, rustCbor.encodeToByteArray(goldenRequest).toHex())

    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<DownloadProgressPlan>>(RESPONSE_CBOR_HEX.decodeHex())
    assertEquals(1, envelope.version)
    assertEquals("downloadProgress", envelope.module)
    assertEquals(goldenPlan, envelope.payload)
    assertEquals(null, envelope.error)
  }

  @Test
  fun estimateRequestCborMatchesRustWithRuntimeNegativeCounters() {
    val request =
      DownloadProgressRequest(
        version = 1,
        snapshot =
          DownloadProgressSnapshot(
            state = DownloadState.DOWNLOADING,
            speedBytesPerSecond = -1,
            remainingMilliseconds = -1,
            finished = 0,
            downloaded = 0,
            total = 10,
            legacy = -1,
          ),
        command =
          DownloadProgressCommand.estimate(
            speedBytesPerSecond = 8_192,
            transfers = listOf(ActiveDownloadTransfer(contentLength = -1, receivedSize = 4_096)),
          ),
      )

    assertEquals(ESTIMATE_REQUEST_CBOR_HEX, rustCbor.encodeToByteArray(request).toHex())
  }

  @Test
  fun nativeCallbackReceivesTheFixedRequestAndReturnsTheFixedResponseOnce() {
    var calls = 0
    val actual =
      planDownloadProgress(goldenRequest) { requestBuffer, requestSize, responseBuffer ->
        calls += 1
        val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
        assertEquals(REQUEST_CBOR_HEX, requestBytes.toHex())
        assertEquals(
          goldenRequest,
          rustCbor.decodeFromByteArray<DownloadProgressRequest>(requestBytes),
        )
        val responseBytes = RESPONSE_CBOR_HEX.decodeHex()
        responseBuffer.put(responseBytes)
        responseBytes.size
      }

    assertEquals(goldenPlan, actual)
    assertEquals(1, calls)
  }

  @Test
  fun bridgeRejectsBadMetadataTypedFailureAndMissingPayload() {
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(ApiEnvelope(2, "downloadProgress", goldenPlan))
    }
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(ApiEnvelope(1, "other", goldenPlan))
    }
    assertFailsWith<RustFailureException> {
      callWithEnvelope(
        ApiEnvelope(
          version = 1,
          module = "downloadProgress",
          error = RustError(RustErrorKind.ERROR, "fixed failure"),
        )
      )
    }
    assertFails {
      callWithResponse(MISSING_PAYLOAD_CBOR_HEX.decodeHex())
    }
  }

  @Test
  fun taskIdentityRequiresTheSameCapturedObject() {
    val expected = Any()

    assertTrue(isExpectedDownloadProgressTask(expected, expected))
    assertFalse(isExpectedDownloadProgressTask(Any(), expected))
    assertFalse(isExpectedDownloadProgressTask(expected, null))
    assertFalse(isExpectedDownloadProgressTask(null, expected))
  }

  private fun callWithEnvelope(envelope: ApiEnvelope<DownloadProgressPlan>) =
    callWithResponse(rustCbor.encodeToByteArray(envelope))

  private fun callWithResponse(response: ByteArray) =
    planDownloadProgress(goldenRequest) { _: ByteBuffer, _: Int, output: ByteBuffer ->
      output.put(response)
      response.size
    }

  private fun ByteArray.toHex() =
    joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

  private companion object {
    const val REQUEST_CBOR_HEX =
      "a36776657273696f6e0168736e617073686f74a7657374617465646e6f6e6573737065656442797465735065725365636f6e64077572656d61696e696e674d696c6c697365636f6e6473096866696e6973686564016a646f776e6c6f616465640265746f74616c03666c65676163790467636f6d6d616e64a4646b696e646d73657450616765526573756c746866696e6973686564036a646f776e6c6f616465640465746f74616c0a"
    const val ESTIMATE_REQUEST_CBOR_HEX =
      "a36776657273696f6e0168736e617073686f74a76573746174656b646f776e6c6f6164696e6773737065656442797465735065725365636f6e64207572656d61696e696e674d696c6c697365636f6e6473206866696e6973686564006a646f776e6c6f616465640065746f74616c0a666c65676163792067636f6d6d616e64a3646b696e6468657374696d61746573737065656442797465735065725365636f6e64192000697472616e736665727381a26d636f6e74656e744c656e677468206c726563656976656453697a65191000"
    const val RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c6570646f776e6c6f616450726f6772657373677061796c6f6164a168736e617073686f74a7657374617465646e6f6e6573737065656442797465735065725365636f6e64077572656d61696e696e674d696c6c697365636f6e6473096866696e6973686564036a646f776e6c6f616465640465746f74616c0a666c656761637904656572726f72f6"
    const val MISSING_PAYLOAD_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c6570646f776e6c6f616450726f6772657373677061796c6f6164f6656572726f72f6"

    val runtimeSnapshot =
      DownloadProgressSnapshot(
        state = DownloadState.DOWNLOADING,
        speedBytesPerSecond = 11,
        remainingMilliseconds = 22,
        finished = 1,
        downloaded = 2,
        total = 10,
        legacy = 8,
      )
    val goldenSnapshot =
      DownloadProgressSnapshot(
        state = DownloadState.NONE,
        speedBytesPerSecond = 7,
        remainingMilliseconds = 9,
        finished = 1,
        downloaded = 2,
        total = 3,
        legacy = 4,
      )
    val goldenCommand =
      DownloadProgressCommand.setPageResult(finished = 3, downloaded = 4, total = 10)
    val goldenRequest = DownloadProgressRequest(1, goldenSnapshot, goldenCommand)
    val goldenPlan =
      DownloadProgressPlan(goldenSnapshot.copy(finished = 3, downloaded = 4, total = 10))
  }
}
