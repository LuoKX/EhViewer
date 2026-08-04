package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveDownloadTarget
import com.hippo.ehviewer.bridge.ArchiveListPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.parser.Archive
import com.hippo.ehviewer.client.parser.ArchiveParser
import com.hippo.ehviewer.client.parser.Funds
import java.net.URI
import java.net.URLDecoder

/** Validates and adapts the V1 archive-list response to the existing Android model. */
internal fun ApiEnvelope<ArchiveListPage>.toArchiveResult(
  externalFunds: Funds?
): ArchiveParser.Result {
  validateArchiveEnvelope()
  error?.let { throw RustFailureException(it) }

  val page = checkNotNull(payload) { "Rust Gallery Archive response has no payload" }
  val funds =
    if (externalFunds != null) {
      require(page.funds == null) { "Rust Gallery Archive returned duplicate funds" }
      externalFunds
    } else {
      val parsed = checkNotNull(page.funds) { "Rust Gallery Archive response has no funds" }
      require(parsed.gp >= 0 && parsed.credit >= 0) {
        "Rust Gallery Archive funds must not be negative"
      }
      Funds(parsed.gp, parsed.credit)
    }
  val options =
    page.options.map { option ->
      require(option.resolution.isNotEmpty()) { "Gallery Archive resolution must not be empty" }
      require(option.size.isNotEmpty()) { "Gallery Archive size must not be empty" }
      require(option.cost.isNotEmpty()) { "Gallery Archive cost must not be empty" }
      require(option.isHath == option.name.isNotEmpty()) {
        "Gallery Archive name must be present exactly for H@H choices"
      }
      Archive(
        res = option.resolution,
        name = option.name,
        size = option.size,
        cost = option.cost,
        isHath = option.isHath,
      )
    }
  return ArchiveParser.Result(options, funds)
}

/** Validates and extracts the V1 final archive URL without weakening Rust's safety policy. */
internal fun ApiEnvelope<ArchiveDownloadTarget>.toArchiveUrl(): String? {
  validateArchiveEnvelope()
  error?.let { throw RustFailureException(it) }

  val value =
    checkNotNull(payload) { "Rust Gallery Archive response has no payload" }.url ?: return null
  val uri = URI(value)
  require(uri.scheme == "https" && !uri.host.isNullOrEmpty()) {
    "Rust Gallery Archive target must be an absolute HTTPS URL"
  }
  require(uri.rawUserInfo == null) { "Rust Gallery Archive target must not contain credentials" }
  require(uri.rawFragment == null) { "Rust Gallery Archive target must not contain a fragment" }
  val startValues =
    uri.rawQuery
      ?.split('&')
      ?.mapNotNull { query ->
        val separator = query.indexOf('=')
        val rawName = if (separator < 0) query else query.substring(0, separator)
        if (decodeQueryPart(rawName) != "start") return@mapNotNull null
        val rawValue = if (separator < 0) "" else query.substring(separator + 1)
        decodeQueryPart(rawValue)
      }
      .orEmpty()
  require(startValues == listOf("1")) {
    "Rust Gallery Archive target must contain exactly one start=1 query"
  }
  return value
}

private fun ApiEnvelope<*>.validateArchiveEnvelope() {
  require(version == GALLERY_ARCHIVE_API_VERSION) {
    "Unsupported Gallery Archive API version: $version"
  }
  require(module == GALLERY_ARCHIVE_MODULE) { "Unexpected Rust module: $module" }
}

private fun decodeQueryPart(value: String): String = URLDecoder.decode(value, Charsets.UTF_8.name())

private const val GALLERY_ARCHIVE_API_VERSION = 1
private const val GALLERY_ARCHIVE_MODULE = "galleryArchive"
