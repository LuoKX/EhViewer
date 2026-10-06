package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

internal const val MAX_DOWNLOAD_METADATA_ROWS = 32
internal const val MAX_DOWNLOAD_METADATA_STRINGS = 16_384
internal const val MAX_DOWNLOAD_METADATA_TEXT_BYTES = 2 * 1024 * 1024
internal const val MAX_DOWNLOAD_METADATA_WIRE_BYTES = 3 * 1024 * 1024

@Serializable
internal enum class DownloadMetadataPhase {
  @SerialName("observe") OBSERVE,
  @SerialName("patch") PATCH,
}

@Serializable
internal data class DownloadMetadataSnapshot(
  val pages: Int,
  val tagsMissing: Boolean,
  val artistsMissing: Boolean,
  val hasDirectory: Boolean,
)

/** Lists already projected by existing Rust ComicInfo codec; never re-project or deduplicate. */
@Serializable
internal data class DownloadMetadataComicFacts(
  val pages: Int,
  val simpleTags: List<String>?,
  val penciller: List<String>?,
)

@Serializable
internal data class DownloadMetadataObservation(
  val index: Int,
  val comic: DownloadMetadataComicFacts? = null,
  val spiderPages: Int?,
)

@Serializable
internal data class DownloadMetadataRequest(
  val version: Int,
  val phase: DownloadMetadataPhase,
  val entries: List<DownloadMetadataSnapshot>,
  val observations: List<DownloadMetadataObservation>,
)

@Serializable
internal data class DownloadMetadataRead(
  val index: Int,
  val spiderIfComicMissing: Boolean,
)

@Serializable
internal data class DownloadMetadataPatch(
  val index: Int,
  val pages: Int?,
  val updateTags: Boolean,
  val simpleTags: List<String>?,
  val artists: List<String>?,
  val persistGallery: Boolean,
  val persistArtists: Boolean,
)

@Serializable
internal data class DownloadMetadataPlan(
  val reads: List<DownloadMetadataRead>,
  val patches: List<DownloadMetadataPatch>,
)

internal typealias DownloadMetadataNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

/** One bounded JNI phase. Validate entire result before directory, row or Room effects. */
internal fun planDownloadMetadata(
  request: DownloadMetadataRequest,
  nativeCall: DownloadMetadataNativeCall = ::nativePlanDownloadMetadataV1,
): DownloadMetadataPlan {
  validateRequest(request)
  val bytes = rustCbor.encodeToByteArray(request)
  require(bytes.size in 1..MAX_DOWNLOAD_METADATA_WIRE_BYTES) {
    "Invalid download metadata request size"
  }
  val envelope =
    invokeRustCbor<ApiEnvelope<DownloadMetadataPlan>>(
      bytes,
      "download metadata",
    ) { input, size, output ->
      nativeCall(input, size, output).also {
        require(it in 1..MAX_DOWNLOAD_METADATA_WIRE_BYTES) {
          "Invalid download metadata response size"
        }
      }
    }
  require(envelope.version == 1 && envelope.module == "downloadMetadata") {
    "Invalid download metadata envelope"
  }
  require(envelope.error == null || envelope.payload == null) {
    "Invalid download metadata envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  val plan = requireNotNull(envelope.payload) { "Missing download metadata plan" }
  validatePlan(request, plan)
  return plan
}

/** Validate source slots, not Rust's choice of eligible rows or observation priority. */
private fun validateIndices(indices: List<Int>, sourceCount: Int) {
  require(
    indices.size <= sourceCount &&
      indices.all { it in 0 until sourceCount } &&
      indices.zipWithNext().all { (left, right) -> left < right }
  ) {
    "Invalid download metadata source indices"
  }
}

private fun validateRequest(request: DownloadMetadataRequest) {
  require(request.version == 1 && request.entries.size <= MAX_DOWNLOAD_METADATA_ROWS) {
    "Invalid download metadata input"
  }
  if (request.phase == DownloadMetadataPhase.OBSERVE) {
    require(request.observations.isEmpty()) { "Invalid download metadata observations" }
  } else {
    validateIndices(request.observations.map { it.index }, request.entries.size)
    request.observations.forEach { fact ->
      require(request.entries[fact.index].hasDirectory) {
        "Download metadata observation has no directory"
      }
      require(fact.comic == null || fact.spiderPages == null) {
        "Conflicting download metadata sidecar facts"
      }
    }
  }
  val strings =
    request.observations
      .asSequence()
      .mapNotNull { it.comic }
      .flatMap {
        (it.simpleTags.orEmpty() + it.penciller.orEmpty()).asSequence()
      }
  var count = 0
  var bytes = 0L
  strings.forEach {
    count++
    bytes += it.toByteArray(Charsets.UTF_8).size
    require(count <= MAX_DOWNLOAD_METADATA_STRINGS && bytes <= MAX_DOWNLOAD_METADATA_TEXT_BYTES) {
      "Download metadata exceeds resource budget"
    }
  }
}

private fun validatePlan(
  request: DownloadMetadataRequest,
  plan: DownloadMetadataPlan,
) {
  if (request.phase == DownloadMetadataPhase.OBSERVE) {
    require(plan.patches.isEmpty()) { "Unexpected download metadata patches" }
    validateIndices(plan.reads.map { it.index }, request.entries.size)
    require(plan.reads.all { request.entries[it.index].hasDirectory }) {
      "Download metadata read has no directory"
    }
    return
  }
  require(plan.reads.isEmpty()) { "Unexpected download metadata reads" }
  validateIndices(plan.patches.map { it.index }, request.entries.size)
  require(plan.patches.size <= request.observations.size) {
    "Invalid download metadata patch count"
  }
  val facts = request.observations.associateBy { it.index }
  plan.patches.forEach { patch ->
    val entry = request.entries[patch.index]
    val fact = requireNotNull(facts[patch.index]) { "Unobserved download metadata source" }
    val comic = fact.comic
    require(
      patch.persistGallery == (patch.pages != null || patch.updateTags) &&
        patch.persistArtists == (patch.artists != null) &&
        (patch.persistGallery || patch.persistArtists) &&
        (patch.updateTags || patch.simpleTags == null)
    ) {
      "Inconsistent download metadata patch"
    }
    // Authorize only selected writes. Rust alone decides whether/how many to select.
    require(!patch.persistGallery || entry.pages == 0 || entry.tagsMissing) {
      "Unauthorized download metadata gallery write"
    }
    require(!patch.persistArtists || entry.artistsMissing) {
      "Unauthorized download metadata artist write"
    }
    patch.pages?.let {
      require(it == (comic?.pages ?: fact.spiderPages)) {
        "Invalid download metadata page provenance"
      }
    }
    if (patch.updateTags) {
      require(comic != null && patch.simpleTags == comic.simpleTags) {
        "Invalid download metadata tag provenance"
      }
    }
    patch.artists?.let {
      require(comic != null && it == comic.penciller) {
        "Invalid download metadata artist provenance"
      }
    }
  }
}

private external fun nativePlanDownloadMetadataV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int
