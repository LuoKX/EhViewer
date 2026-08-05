package com.hippo.ehviewer.bridge.home

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveFunds
import com.hippo.ehviewer.bridge.ImageLimitsReset
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentKey
import com.hippo.ehviewer.client.exception.ParseException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class HomeResponseBridgeTest {
  @Test
  fun adaptsGoodAndBoundaryPayloadsWithoutNormalization() {
    ApiEnvelope(
        HOME_RESPONSE_API_VERSION,
        HOME_RESET_LIMITS_MODULE,
        ImageLimitsReset(accepted = true),
      )
      .toLegacyResetLimits()

    assertEquals(
      " raw key ",
      ApiEnvelope(
          HOME_RESPONSE_API_VERSION,
          HOME_TORRENT_KEY_MODULE,
          TorrentKey(" raw key "),
        )
        .toLegacyTorrentKey(),
    )
    assertEquals(
      "",
      ApiEnvelope(HOME_RESPONSE_API_VERSION, HOME_TORRENT_KEY_MODULE, TorrentKey(""))
        .toLegacyTorrentKey(),
    )
    assertEquals(
      5_678 to 1_234,
      ApiEnvelope(
          HOME_RESPONSE_API_VERSION,
          HOME_FUNDS_MODULE,
          ArchiveFunds(gp = 5_678, credit = 1_234),
        )
        .toLegacyFunds()
        .let { it.gp to it.credit },
    )
  }

  @Test
  fun preservesLegacyBusinessExceptionMapping() {
    val insufficient =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<ImageLimitsReset>(
            HOME_RESPONSE_API_VERSION,
            HOME_RESET_LIMITS_MODULE,
            error = RustError(RustErrorKind.INSUFFICIENT_FUNDS),
          )
          .toLegacyResetLimits()
      }
    assertEquals(RustErrorKind.INSUFFICIENT_FUNDS, insufficient.failure.kind)
    assertFailsWith<ParseException> {
      ApiEnvelope<TorrentKey>(
          HOME_RESPONSE_API_VERSION,
          HOME_TORRENT_KEY_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toLegacyTorrentKey()
    }
    assertFailsWith<ParseException> {
      ApiEnvelope<ArchiveFunds>(
          HOME_RESPONSE_API_VERSION,
          HOME_FUNDS_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toLegacyFunds()
    }
  }

  @Test
  fun rejectsVersionModuleAndPayloadShapeDrift() {
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(2, HOME_TORRENT_KEY_MODULE, TorrentKey("key")).toLegacyTorrentKey()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(HOME_RESPONSE_API_VERSION, "other", ArchiveFunds(1, 2)).toLegacyFunds()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          HOME_RESPONSE_API_VERSION,
          HOME_RESET_LIMITS_MODULE,
          ImageLimitsReset(accepted = false),
        )
        .toLegacyResetLimits()
    }
  }
}
