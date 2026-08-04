package com.hippo.ehviewer.bridge.auth

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates a V1 `eh_net::auth` envelope and returns its success payload. */
internal fun <T> ApiEnvelope<T>.requireAuthPayload(): T {
  require(version == AUTH_API_VERSION) { "Unsupported Auth API version: $version" }
  require(module == AUTH_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Auth response has no payload" }
}

private const val AUTH_API_VERSION = 1
private const val AUTH_MODULE = "auth"
