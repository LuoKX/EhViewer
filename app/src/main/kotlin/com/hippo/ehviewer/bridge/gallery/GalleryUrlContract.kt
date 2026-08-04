package com.hippo.ehviewer.bridge.gallery

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Sanitized gallery host shared by versioned `eh_net::gallery` URL builders. */
@Serializable
internal enum class GalleryUrlSite {
  @SerialName("public") PUBLIC,
  @SerialName("exHentai") EX_HENTAI,
}

/**
 * Fully encoded request target returned by a versioned `eh_net::gallery` URL protocol.
 *
 * @property url HTTPS URL whose path, query encoding, and parameter order have been finalized.
 */
@Serializable internal data class GalleryUrlPlan(val url: String)

/** Ordered form field shared by gallery request planners. */
@Serializable internal data class GalleryFormField(val name: String, val value: String)
