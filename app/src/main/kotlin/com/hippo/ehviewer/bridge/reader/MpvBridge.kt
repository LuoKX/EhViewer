package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.MpvTokens
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates the V1 MPV envelope while preserving every source token value. */
internal fun ApiEnvelope<MpvTokens>.toPTokenList(): List<String> {
  require(version == MPV_API_VERSION) { "Unsupported Reader MPV API version: $version" }
  require(module == MPV_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Reader MPV response has no payload" }.tokens
}

private const val MPV_API_VERSION = 1
private const val MPV_MODULE = "readerMpv"
