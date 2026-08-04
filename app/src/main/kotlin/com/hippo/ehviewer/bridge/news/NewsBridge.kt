package com.hippo.ehviewer.bridge.news

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.NewsPage
import com.hippo.ehviewer.bridge.RustFailureException

/** Validates and adapts the V1 `eh_net::news` event-pane response. */
internal fun ApiEnvelope<NewsPage>.toEventHtml(): String? {
  require(version == NEWS_API_VERSION) { "Unsupported News API version: $version" }
  require(module == NEWS_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust News response has no payload" }.eventHtml
}

private const val NEWS_API_VERSION = 1
private const val NEWS_MODULE = "news"
