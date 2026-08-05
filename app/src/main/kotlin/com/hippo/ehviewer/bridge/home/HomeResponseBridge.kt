package com.hippo.ehviewer.bridge.home

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveFunds
import com.hippo.ehviewer.bridge.ImageLimitsReset
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentKey
import com.hippo.ehviewer.client.exception.ParseException
import com.hippo.ehviewer.client.parser.Funds

/** Validates one Rust-owned image-limit reset response for the shared Android error mapper. */
internal fun ApiEnvelope<ImageLimitsReset>.toLegacyResetLimits() {
  validateHomeEnvelope(HOME_RESET_LIMITS_MODULE)
  error?.let { failure -> throw RustFailureException(failure) }
  require(checkNotNull(payload) { "Rust reset-limits response has no payload" }.accepted) {
    "Rust reset-limits response was not accepted"
  }
}

/** Validates one Rust-owned torrent-key response and returns its raw capture. */
internal fun ApiEnvelope<TorrentKey>.toLegacyTorrentKey(): String {
  validateHomeEnvelope(HOME_TORRENT_KEY_MODULE)
  error?.let { failure -> throwHomeParseFailure(failure, "Parse torrent key error") }
  return checkNotNull(payload) { "Rust torrent-key response has no payload" }.key
}

/** Validates one Rust-owned funds response and preserves `(gp, credit)` field ordering. */
internal fun ApiEnvelope<ArchiveFunds>.toLegacyFunds(): Funds {
  validateHomeEnvelope(HOME_FUNDS_MODULE)
  error?.let { failure -> throwHomeParseFailure(failure, "Parse funds error") }
  val funds = checkNotNull(payload) { "Rust funds response has no payload" }
  return Funds(gp = funds.gp, credit = funds.credit)
}

private fun ApiEnvelope<*>.validateHomeEnvelope(expectedModule: String) {
  require(version == HOME_RESPONSE_API_VERSION) { "Unsupported Home response API version" }
  require(module == expectedModule) { "Unexpected Home response module: $module" }
}

private fun throwHomeParseFailure(failure: RustError, message: String): Nothing {
  if (failure.kind == RustErrorKind.ERROR) throw ParseException(message)
  throw RustFailureException(failure)
}

internal const val HOME_RESPONSE_API_VERSION = 1
internal const val HOME_RESET_LIMITS_MODULE = "homeResetLimits"
internal const val HOME_TORRENT_KEY_MODULE = "homeTorrentKey"
internal const val HOME_FUNDS_MODULE = "homeFunds"
