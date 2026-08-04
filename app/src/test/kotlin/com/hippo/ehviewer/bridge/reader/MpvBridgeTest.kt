package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.MpvTokens
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class MpvBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixtureWithoutChangingTokens() {
    val envelope = Cbor.decodeFromByteArray<ApiEnvelope<MpvTokens>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(listOf("0123456789", "abcdef0123"), envelope.toPTokenList())
  }

  @Test
  fun preservesEmptyDuplicateAndEmptyStringTokens() {
    val tokens = listOf("", "duplicate", "duplicate")
    assertEquals(tokens, successEnvelope(tokens).toPTokenList())
    assertEquals(emptyList(), successEnvelope(emptyList()).toPTokenList())
  }

  @Test
  fun rejectsInvalidContractsAndTypedFailures() {
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(emptyList()).copy(version = 2).toPTokenList()
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(emptyList()).copy(module = "mpv").toPTokenList()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<MpvTokens>(version = 1, module = "readerMpv").toPTokenList()
    }

    val failure = RustError(RustErrorKind.ERROR, "MPV image list is invalid")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<MpvTokens>(version = 1, module = "readerMpv", error = failure).toPTokenList()
      }
    assertEquals(failure, error.failure)
  }

  private fun successEnvelope(tokens: List<String>) =
    ApiEnvelope(version = 1, module = "readerMpv", payload = MpvTokens(tokens))

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c65697265616465724d7076677061796c6f6164a166746f" +
        "6b656e73826a303132333435363738396a61626364656630313233656572726f72f6"
  }
}
