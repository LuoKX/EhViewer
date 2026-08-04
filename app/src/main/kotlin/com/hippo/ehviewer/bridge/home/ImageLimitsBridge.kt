package com.hippo.ehviewer.bridge.home

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ImageLimits
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.Limits

/** Validates and adapts the V1 `eh_net::home` image-limit response. */
internal fun ApiEnvelope<ImageLimits>.toLegacyLimits(): Limits {
  require(version == IMAGE_LIMITS_API_VERSION) {
    "Unsupported Image Limits API version: $version"
  }
  require(module == IMAGE_LIMITS_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  val limits = checkNotNull(payload) { "Rust Image Limits response has no payload" }
  return Limits(
    current = limits.current,
    maximum = limits.maximum,
    resetCost = limits.resetCost,
  )
}

private const val IMAGE_LIMITS_API_VERSION = 1
private const val IMAGE_LIMITS_MODULE = "imageLimits"
