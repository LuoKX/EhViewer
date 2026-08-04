package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.client.exception.EhError
import kotlin.test.Test
import kotlin.test.assertEquals

class RustFailureTest {
  @Test
  fun mapsEveryV1ErrorKindToTheExistingAndroidContract() {
    val cases =
      listOf(
        RustError(RustErrorKind.NO_HITS) to EhError.NoHits,
        RustError(RustErrorKind.NO_WATCHED) to EhError.NoWatched,
        RustError(RustErrorKind.NEED_LOGIN) to EhError.NeedLogin,
        RustError(RustErrorKind.NO_HATH_CLIENT) to EhError.NoHathClient,
        RustError(RustErrorKind.INSUFFICIENT_FUNDS) to EhError.InsufficientFunds,
        RustError(RustErrorKind.GALLERY_UNAVAILABLE, "gone") to EhError.GalleryUnavailable("gone"),
        RustError(RustErrorKind.IP_BANNED, "blocked") to EhError.IpBanned("blocked"),
        RustError(RustErrorKind.ERROR, "failure") to EhError.Error("failure"),
      )

    cases.forEach { (rust, android) ->
      assertEquals(android, rust.toLegacyError())
      assertEquals(rust, android.toRustError())
    }
  }
}
