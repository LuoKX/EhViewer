package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 pure operation owned by `eh_domain::gallery_enrichment`. */
@Serializable
internal enum class GalleryEnrichmentKind {
  @SerialName("language") LANGUAGE,
  @SerialName("metadataNeeded") METADATA_NEEDED,
}

/** V1 facts observed by Android before extra Gallery metadata HTTP. */
@Serializable
internal data class GalleryMetadataFacts(
  val filter: Boolean,
  val needsTags: Boolean,
  val hasTags: Boolean,
  val showPages: Boolean,
  val hasPages: Boolean,
  val hasRated: Boolean,
)

/** V1 tag/title input or metadata eligibility snapshot; never credentials. */
@Serializable
internal data class GalleryEnrichmentRequest(
  val version: Int,
  val kind: GalleryEnrichmentKind,
  val title: String? = null,
  val tags: List<String>? = null,
  val facts: GalleryMetadataFacts? = null,
)

/** V1 validated result of one complete enrichment decision. */
@Serializable
internal data class GalleryEnrichmentPlan(
  val kind: GalleryEnrichmentKind,
  val language: String?,
  val needsMetadata: Boolean?,
)

internal fun planGalleryEnrichment(
  request: GalleryEnrichmentRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanGalleryEnrichmentV1,
): GalleryEnrichmentPlan {
  val envelope =
    invokeRustCbor<ApiEnvelope<GalleryEnrichmentPlan>>(
      rustCbor.encodeToByteArray(request),
      operation = "Gallery enrichment",
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "galleryEnrichment") {
    "Unexpected gallery enrichment contract"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = checkNotNull(envelope.payload) { "Missing gallery enrichment payload" }
  require(plan.kind == request.kind) { "Unexpected gallery enrichment operation" }
  when (plan.kind) {
    GalleryEnrichmentKind.LANGUAGE ->
      require(
        plan.needsMetadata == null && (plan.language == null || plan.language in LANGUAGE_CODES)
      ) {
        "Invalid gallery language plan"
      }
    GalleryEnrichmentKind.METADATA_NEEDED ->
      require(plan.language == null && plan.needsMetadata != null) {
        "Invalid gallery eligibility plan"
      }
  }
  return plan
}

/** Delegates all tag/title inference to Rust; Android performs only model assignment. */
internal fun inferGalleryLanguage(title: String?, tags: List<String>?): String? =
  planGalleryEnrichment(
      GalleryEnrichmentRequest(1, GalleryEnrichmentKind.LANGUAGE, title, tags?.toList())
    )
    .language

/** Delegates the complete extra-request rule to Rust using one settings/filter snapshot. */
internal fun needsGalleryMetadata(facts: GalleryMetadataFacts): Boolean =
  checkNotNull(
    planGalleryEnrichment(
        GalleryEnrichmentRequest(1, GalleryEnrichmentKind.METADATA_NEEDED, facts = facts)
      )
      .needsMetadata
  )

/** V1 `eh_domain::gallery_enrichment` CBOR planner. */
private external fun nativePlanGalleryEnrichmentV1(
  request: ByteBuffer,
  size: Int,
  response: ByteBuffer,
): Int

private val LANGUAGE_CODES =
  setOf("EN", "ZH", "ES", "KO", "RU", "FR", "PT", "TH", "DE", "IT", "VI", "PL", "HU", "NL")
