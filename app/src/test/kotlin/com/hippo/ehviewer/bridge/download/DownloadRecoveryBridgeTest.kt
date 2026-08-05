package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.encodeToByteArray

class DownloadRecoveryBridgeTest {
  @Test
  fun metadataBytesMatchTheRustCborByteStringGolden() {
    val bytes = byteArrayOf(0, 0x80.toByte(), 0xff.toByte())
    val request =
      RecoveryCandidateRequest(
        version = 1,
        source = RecoverySourceKeyInput("r", "s", RecoverySourceKind.DIRECTORY),
        fingerprint = RecoveryFingerprintInput(contentDigest = bytes),
        displayName = "d",
        spiderInfo = bytes,
        comicInfo = bytes,
      )

    assertEquals(
      KOTLIN_BYTE_STRING_REQUEST_CBOR_HEX,
      rustCbor.encodeToByteArray(request).toHex(),
    )
  }

  private fun ByteArray.toHex() =
    joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

  private companion object {
    const val KOTLIN_BYTE_STRING_REQUEST_CBOR_HEX =
      "a66776657273696f6e0166736f75726365a366726f6f744964617268736f75726365496461736a736f757263654b696e64696469726563746f72796b66696e6765727072696e74a16d636f6e74656e74446967657374430080ff6b646973706c61794e616d6561646a737069646572496e666f430080ff69636f6d6963496e666f430080ff"
  }
}
