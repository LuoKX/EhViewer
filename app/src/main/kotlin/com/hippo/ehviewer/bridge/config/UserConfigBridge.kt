package com.hippo.ehviewer.bridge.config

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.FavoriteCategories
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates and adapts the V1 `eh_net::user_config` response. */
internal fun ApiEnvelope<FavoriteCategories>.toFavoriteCategoryArray(): Array<String> {
  require(version == USER_CONFIG_API_VERSION) {
    "Unsupported User Config API version: $version"
  }
  require(module == USER_CONFIG_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  // Do not enforce the ten-item settings invariant here. The legacy Rust parser
  // also returns empty/partial arrays; Settings.favCat remains the application boundary.
  return checkNotNull(payload) { "Rust User Config response has no payload" }.names.toTypedArray()
}

private const val USER_CONFIG_API_VERSION = 1
private const val USER_CONFIG_MODULE = "userConfig"
