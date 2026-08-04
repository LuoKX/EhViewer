package com.hippo.ehviewer.bridge.auth

import com.hippo.ehviewer.bridge.AccountProfile
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.SignInIdentity
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class AuthBridgeTest {
  @Test
  fun returnsSignInAndProfilePayloads() {
    val identity =
      ApiEnvelope(
          version = 1,
          module = "auth",
          payload = SignInIdentity("Reader"),
        )
        .requireAuthPayload()
    assertEquals("Reader", identity.username)

    val profile =
      ApiEnvelope(
          version = 1,
          module = "auth",
          payload = AccountProfile("Reader & Artist", null),
        )
        .requireAuthPayload()
    assertEquals("Reader & Artist", profile.displayName)
    assertEquals(null, profile.avatar)
  }

  @Test
  fun decodesRustGeneratedIdentityAndProfileGoldenFixtures() {
    val identity =
      Cbor.decodeFromByteArray<ApiEnvelope<SignInIdentity>>(IDENTITY_CBOR_HEX.decodeHex())
        .requireAuthPayload()
    val profile =
      Cbor.decodeFromByteArray<ApiEnvelope<AccountProfile>>(PROFILE_CBOR_HEX.decodeHex())
        .requireAuthPayload()

    assertEquals("Reader", identity.username)
    assertEquals("Reader & Artist", profile.displayName)
    assertEquals("https://forums.e-hentai.org/ehgt/avatar.png", profile.avatar)
  }

  @Test
  fun rejectsUnsupportedVersionUnexpectedModuleAndTypedFailure() {
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "auth", payload = SignInIdentity("Reader"))
        .requireAuthPayload()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = SignInIdentity("Reader"))
        .requireAuthPayload()
    }

    val failure = RustError(RustErrorKind.NEED_LOGIN)
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<SignInIdentity>(version = 1, module = "auth", error = failure)
          .requireAuthPayload()
      }
    assertEquals(failure, error.failure)
  }

  private companion object {
    const val IDENTITY_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656461757468677061796c6f6164a168757365726e616d" +
        "6566526561646572656572726f72f6"
    const val PROFILE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656461757468677061796c6f6164a26b646973706c6179" +
        "4e616d656f52656164657220262041727469737466617661746172782b68747470733a2f2f666f72" +
        "756d732e652d68656e7461692e6f72672f656867742f6176617461722e706e67656572726f72f6"
  }
}
