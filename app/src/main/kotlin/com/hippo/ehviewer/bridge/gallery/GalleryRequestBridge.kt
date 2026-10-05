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
}

/** V1 public identity in one source-ordered metadata batch. */
@Serializable internal data class GalleryRequestIdentity(val gid: Long, val token: String)

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
)

/** V1 ordered form field selected by Rust. */
@Serializable internal data class GalleryRequestFormField(val name: String, val value: String)

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
)

internal fun planGalleryRequest(
  request: GalleryRequest,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativePlanGalleryRequestV1,
): GalleryRequestPlan {
  val envelope =
    invokeRustCborBytes<ApiEnvelope<GalleryRequestPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery request",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "galleryRequest") {
    "Unexpected gallery request contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing gallery request payload" }
  require(
    plan.kind == request.kind &&
      plan.url.isNotBlank() &&
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
    if (isUrl)
      plan.form.isEmpty() && plan.body == null && plan.origin == null && plan.referer == null
    else if (isComment) plan.form.isNotEmpty() && plan.body == null
    else plan.form.isEmpty() && plan.body != null
  ) {
    "Invalid gallery transport shape"
  }
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

private val URL_KINDS =
  setOf(GalleryRequestKind.DETAIL_URL, GalleryRequestKind.PAGE_URL, GalleryRequestKind.MPV_URL)
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
private external fun nativePlanGalleryRequestV1(request: ByteBuffer, size: Int): ByteArray
