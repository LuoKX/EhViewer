@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.client.formBody
import com.hippo.ehviewer.client.json
import com.hippo.ehviewer.client.jsonBody
import io.ktor.client.request.HttpRequestBuilder
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** V1 complete request operation owned by `eh_net::gallery::request`. */
@Serializable
internal enum class GalleryRequestKind {
  @SerialName("comment") COMMENT,
  @SerialName("showPage") SHOW_PAGE,
  @SerialName("rate") RATE,
  @SerialName("metadata") METADATA,
  @SerialName("voteComment") VOTE_COMMENT,
  @SerialName("voteTag") VOTE_TAG,
  @SerialName("token") TOKEN,
  @SerialName("detailUrl") DETAIL_URL,
  @SerialName("pageUrl") PAGE_URL,
  @SerialName("mpvUrl") MPV_URL,
  @SerialName("addFavoritesUrl") ADD_FAVORITES_URL,
  @SerialName("torrentUrl") TORRENT_URL,
  @SerialName("trackerUrl") TRACKER_URL,
  @SerialName("tagDefinitionUrl") TAG_DEFINITION_URL,
}

/** Convert Android's setting snapshot into the closed wire site, retaining unknown -> public. */
internal fun galleryRequestSite(gallerySite: Int): GalleryUrlSite =
  if (gallerySite == 1) GalleryUrlSite.EX_HENTAI else GalleryUrlSite.PUBLIC

/** V1 public identity in one source-ordered metadata batch. */
@Serializable
internal data class GalleryRequestIdentity(
  val gid: Long,
  val token: String,
)

/** V1 portable input. API keys, API user IDs and cookies deliberately never cross JNI. */
@Serializable
internal data class GalleryRequest(
  val version: Int,
  val site: GalleryUrlSite,
  val kind: GalleryRequestKind,
  val gid: Long = 0,
  val token: String = "",
  val index: Int = 0,
  val allComments: Boolean = false,
  val nl: String? = null,
  val url: String? = null,
  val comment: String? = null,
  val commentId: Long? = null,
  val rating: Float = 0f,
  val showKey: String? = null,
  val previousToken: String? = null,
  val identities: List<GalleryRequestIdentity> = emptyList(),
  val vote: Int = 0,
  val tags: String = "",
  val key: String? = null,
  @ByteString val wikiTagUtf16: ByteArray? = null,
)

/** V1 ordered form field selected by Rust. */
@Serializable
internal data class GalleryRequestFormField(
  val name: String,
  val value: String,
)

/** V1 complete credential-free plan. Android interprets transport data, not request rules. */
@Serializable
internal data class GalleryRequestPlan(
  val kind: GalleryRequestKind,
  val url: String,
  val referer: String?,
  val origin: String?,
  val form: List<GalleryRequestFormField>,
  val body: String?,
  val credentials: Boolean,
  @ByteString val wikiUrlUtf16: ByteArray? = null,
  val wikiSourceIndex: Int? = null,
)

internal fun planGalleryRequest(
  request: GalleryRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativePlanGalleryRequestV1,
): GalleryRequestPlan {
  val isNavigation = request.kind in REMAINING_NAVIGATION_KINDS
  val isWiki = request.kind == GalleryRequestKind.TAG_DEFINITION_URL
  require(request.version == 1 && request.identities.size <= 250) {
    "Invalid gallery request input"
  }
  if (isWiki) {
    val tag = request.wikiTagUtf16
    require(
      tag != null &&
        tag.size <= WIKI_UTF16_BYTE_BUDGET &&
        tag.size % 2 == 0 &&
        request.token.isEmpty() &&
        request.key == null
    ) {
      "Invalid gallery wiki request shape"
    }
  } else {
    require(request.wikiTagUtf16 == null) { "Invalid gallery wiki request shape" }
    if (isNavigation) {
      require(
        request.token.toByteArray().size.toLong() + (request.key?.toByteArray()?.size ?: 0) <=
          NAVIGATION_TEXT_BUDGET
      ) {
        "Gallery navigation exceeds text budget"
      }
    }
  }
  val encoded = rustCbor.encodeToByteArray(request)
  require(encoded.size <= REQUEST_BYTE_BUDGET) { "Gallery request exceeds input budget" }
  val envelope =
    try {
      invokeRustCborBytes<ApiEnvelope<GalleryRequestPlan>>(
        encoded,
        operation = "Gallery request",
        maxResponseSize = if (isNavigation) NAVIGATION_RESPONSE_BUDGET else RESPONSE_BYTE_BUDGET,
        nativeCall = nativeCall,
      )
    } catch (_: SerializationException) {
      // Decoder diagnostics can contain untrusted URL/key/enum text.
      throw IllegalArgumentException("Invalid gallery request response")
    }
  require(envelope.version == 1 && envelope.module == "galleryRequest") {
    "Unexpected gallery request contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing gallery request payload" }
  require(
    plan.kind == request.kind &&
      (if (isWiki) plan.url.isEmpty() else plan.url.isNotBlank()) &&
      !(plan.form.isNotEmpty() && plan.body != null)
  ) {
    "Invalid gallery request plan"
  }
  val isUrl = request.kind in URL_KINDS
  val isComment = request.kind == GalleryRequestKind.COMMENT
  require(plan.credentials == (request.kind in CREDENTIAL_KINDS)) {
    "Invalid gallery credential slots"
  }
  require(
    if (isUrl) {
      plan.form.isEmpty() && plan.body == null && plan.origin == null && plan.referer == null
    } else if (isComment) {
      plan.form.isNotEmpty() && plan.body == null
    } else {
      plan.form.isEmpty() && plan.body != null
    }
  ) {
    "Invalid gallery transport shape"
  }
  if (isWiki) return mapWikiTarget(request, plan)
  require(plan.wikiUrlUtf16 == null && plan.wikiSourceIndex == null) {
    "Invalid gallery wiki response shape"
  }
  if (isNavigation) validateNavigationTarget(request, plan.url)
  plan.body?.let { body ->
    val fields = json.parseToJsonElement(body).jsonObject
    require(
      fields["method"]?.jsonPrimitive?.content == API_METHODS[request.kind] &&
        "apiuid" !in fields &&
        "apikey" !in fields
    ) {
      "Invalid gallery JSON plan"
    }
  }
  return plan
}

/** Executes Rust's field plan while keeping credential values entirely Android-owned. */
internal fun HttpRequestBuilder.applyGalleryRequestPlan(
  plan: GalleryRequestPlan,
  apiUid: Long = 0,
  apiKey: String? = null,
) {
  if (plan.form.isNotEmpty()) {
    formBody { plan.form.forEach { append(it.name, it.value) } }
  } else if (plan.body != null) {
    val fields = json.parseToJsonElement(plan.body).jsonObject
    jsonBody {
      fields.forEach { (name, value) ->
        put(name, value)
        if (name == "method" && plan.credentials) {
          put("apiuid", apiUid)
          put("apikey", requireNotNull(apiKey))
        }
      }
    }
  }
}

private const val REQUEST_BYTE_BUDGET = 2 * 1024 * 1024
private const val RESPONSE_BYTE_BUDGET = 16 * 1024 * 1024
private const val NAVIGATION_TEXT_BUDGET = 64 * 1024
private const val WIKI_UTF16_BYTE_BUDGET = 2 * NAVIGATION_TEXT_BUDGET
private const val WIKI_PREFIX = "https://ehwiki.org/wiki/"
private const val NAVIGATION_RESPONSE_BUDGET = 3 * NAVIGATION_TEXT_BUDGET + 256
private val REMAINING_NAVIGATION_KINDS =
  setOf(
    GalleryRequestKind.ADD_FAVORITES_URL,
    GalleryRequestKind.TORRENT_URL,
    GalleryRequestKind.TRACKER_URL,
    GalleryRequestKind.TAG_DEFINITION_URL,
  )
private val URL_KINDS =
  setOf(GalleryRequestKind.DETAIL_URL, GalleryRequestKind.PAGE_URL, GalleryRequestKind.MPV_URL) +
    REMAINING_NAVIGATION_KINDS

/** Validate target/transport structure without reconstructing or encoding the URL. */
private fun validateNavigationTarget(
  request: GalleryRequest,
  url: String,
) {
  val origin =
    if (request.site == GalleryUrlSite.EX_HENTAI) "https://exhentai.org" else "https://e-hentai.org"
  val valid =
    when (request.kind) {
      GalleryRequestKind.ADD_FAVORITES_URL,
      GalleryRequestKind.TORRENT_URL -> {
        val script =
          if (request.kind == GalleryRequestKind.ADD_FAVORITES_URL) {
            "gallerypopups.php"
          } else {
            "gallerytorrents.php"
          }
        val prefix = "$origin/$script?gid=${request.gid}&t="
        val suffix = if (request.kind == GalleryRequestKind.ADD_FAVORITES_URL) "&act=addfav" else ""
        url.startsWith(prefix) &&
          url.endsWith(suffix) &&
          url.length >= prefix.length + suffix.length &&
          validEncodedValue(url.substring(prefix.length, url.length - suffix.length), path = false)
      }

      GalleryRequestKind.TRACKER_URL -> {
        val prefix = "https://ehtracker.org/${request.gid}/"
        if (!url.startsWith(prefix)) {
          false
        } else {
          val path = url.removePrefix(prefix)
          if (request.key == null) {
            path == "announce"
          } else {
            path.endsWith("/announce") &&
              validEncodedValue(path.removeSuffix("/announce"), path = true)
          }
        }
      }

      else -> {
        false
      }
    }
  require(valid) { "Invalid gallery navigation target" }
}

/** Validate raw wire shape first, then map bytes to Java chars without Charset normalization. */
private fun mapWikiTarget(request: GalleryRequest, plan: GalleryRequestPlan): GalleryRequestPlan {
  val source = checkNotNull(request.wikiTagUtf16) { "Missing gallery wiki source" }
  val raw = requireNotNull(plan.wikiUrlUtf16) { "Missing gallery wiki target" }
  val prefixBytes = WIKI_PREFIX.length * 2
  require(
    plan.wikiSourceIndex == 0 &&
      raw.size % 2 == 0 &&
      raw.size == prefixBytes + source.size &&
      raw.size <= prefixBytes + WIKI_UTF16_BYTE_BUDGET &&
      WIKI_PREFIX.indices.all {
        raw[it * 2].toInt() == WIKI_PREFIX[it].code && raw[it * 2 + 1] == 0.toByte()
      }
  ) {
    "Invalid gallery wiki target"
  }
  val chars =
    CharArray(raw.size / 2) { index ->
      ((raw[index * 2].toInt() and 0xff) or ((raw[index * 2 + 1].toInt() and 0xff) shl 8)).toChar()
    }
  // Raw wire fields are consumed; platform callers receive only the reconstructed Java target.
  return plan.copy(url = String(chars), wikiUrlUtf16 = null, wikiSourceIndex = null)
}

// Linear scan avoids regex stack growth for maximum-sized encoded targets.
private fun validEncodedValue(
  value: String,
  path: Boolean,
): Boolean {
  val punctuation = if (path) ":@!$&'()*+,;=-._~" else "+-._~"
  var index = 0
  while (index < value.length) {
    val char = value[index]
    if (char == '%') {
      if (
        index + 2 >= value.length ||
          value[index + 1] !in HEX_DIGITS ||
          value[index + 2] !in HEX_DIGITS
      ) {
        return false
      }
      index += 3
    } else {
      if (char !in 'a'..'z' && char !in 'A'..'Z' && char !in '0'..'9' && char !in punctuation) {
        return false
      }
      index++
    }
  }
  return true
}

private const val HEX_DIGITS = "0123456789ABCDEF"
private val CREDENTIAL_KINDS =
  setOf(GalleryRequestKind.RATE, GalleryRequestKind.VOTE_COMMENT, GalleryRequestKind.VOTE_TAG)
private val API_METHODS =
  mapOf(
    GalleryRequestKind.SHOW_PAGE to "showpage",
    GalleryRequestKind.RATE to "rategallery",
    GalleryRequestKind.METADATA to "gdata",
    GalleryRequestKind.VOTE_COMMENT to "votecomment",
    GalleryRequestKind.VOTE_TAG to "taggallery",
    GalleryRequestKind.TOKEN to "gtoken",
  )

/** V1 exact-sized request-plan response owned by `eh_ffi::jvm::gallery_request`. */
private external fun nativePlanGalleryRequestV1(
  request: ByteBuffer,
  size: Int,
): ByteArray
