package com.hippo.ehviewer.bridge.download

import com.ehviewer.core.database.model.DownloadInfo
import com.ehviewer.core.database.model.artists
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.download.DownloadsFilterMode
import com.hippo.ehviewer.download.SortMode
import com.hippo.ehviewer.ui.screen.DownloadsFilterState
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Immutable display facts for one Android-owned download row. */
@Serializable
internal data class DownloadCatalogEntry(
  @SerialName("galleryId") val galleryId: Long,
  val title: String?,
  @SerialName("titleJpn") val titleJpn: String?,
  val time: Long,
  val pages: Int,
  val label: String?,
  val state: Int,
  val artists: List<String>,
  @SerialName("simpleTags") val simpleTags: List<String>,
) {
  companion object {
    fun from(info: DownloadInfo) =
      DownloadCatalogEntry(
        galleryId = info.gid,
        title = info.title,
        titleJpn = info.titleJpn,
        time = info.time,
        pages = info.pages,
        label = info.label,
        state = info.state,
        artists = info.artists,
        simpleTags = info.simpleTags.orEmpty(),
      )
  }
}

@Serializable
internal enum class DownloadCatalogSortField {
  @SerialName("galleryId") GALLERY_ID,
  @SerialName("title") TITLE,
  @SerialName("time") TIME,
  @SerialName("pages") PAGES,
}

@Serializable
internal enum class DownloadCatalogSortOrder {
  @SerialName("ascending") ASCENDING,
  @SerialName("descending") DESCENDING,
}

/** Versioned request for one complete catalog ordering. */
@Serializable
internal data class DownloadSortRequest(
  val version: Int,
  val entries: List<DownloadCatalogEntry>,
  val field: DownloadCatalogSortField,
  val order: DownloadCatalogSortOrder,
  @SerialName("groupByDownloadLabel") val groupByDownloadLabel: Boolean,
  @SerialName("preferJapaneseTitle") val preferJapaneseTitle: Boolean,
) {
  companion object {
    fun from(entries: List<DownloadInfo>, mode: SortMode, preferJapaneseTitle: Boolean) =
      DownloadSortRequest(
        version = DOWNLOAD_CATALOG_API_VERSION,
        entries = entries.map(DownloadCatalogEntry::from),
        field =
          when (mode.field) {
            SortMode.Field.GID -> DownloadCatalogSortField.GALLERY_ID
            SortMode.Field.TITLE -> DownloadCatalogSortField.TITLE
            SortMode.Field.TIME -> DownloadCatalogSortField.TIME
            SortMode.Field.PAGES -> DownloadCatalogSortField.PAGES
          },
        order =
          when (mode.order) {
            SortMode.Order.ASC -> DownloadCatalogSortOrder.ASCENDING
            SortMode.Order.DESC -> DownloadCatalogSortOrder.DESCENDING
          },
        groupByDownloadLabel = mode.groupByDownloadLabel,
        preferJapaneseTitle = preferJapaneseTitle,
      )
  }
}

@Serializable
internal data class DownloadSortPlan(@SerialName("galleryIds") val galleryIds: List<Long>)

@Serializable
internal enum class DownloadCatalogFilterMode {
  @SerialName("custom") CUSTOM,
  @SerialName("artist") ARTIST,
}

/** Versioned request for one complete catalog filter operation. */
@Serializable
internal data class DownloadFilterRequest(
  val version: Int,
  val entries: List<DownloadCatalogEntry>,
  val mode: DownloadCatalogFilterMode,
  val label: String?,
  val state: Int,
  val keyword: String,
) {
  companion object {
    fun from(entries: List<DownloadInfo>, filter: DownloadsFilterState) =
      DownloadFilterRequest(
        version = DOWNLOAD_CATALOG_API_VERSION,
        entries = entries.map(DownloadCatalogEntry::from),
        mode =
          when (filter.mode) {
            DownloadsFilterMode.CUSTOM -> DownloadCatalogFilterMode.CUSTOM
            DownloadsFilterMode.ARTIST -> DownloadCatalogFilterMode.ARTIST
          },
        label = filter.label,
        state = filter.state,
        keyword = filter.keyword,
      )
  }
}

@Serializable
internal data class DownloadFilterPlan(@SerialName("galleryIds") val galleryIds: List<Long>)

/** Calls Rust once and validates a complete permutation of the request identifiers. */
internal fun sortDownloadCatalog(
  request: DownloadSortRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeSortDownloadCatalogV1,
): DownloadSortPlan {
  val envelope =
    invokeDownloadCatalog<DownloadSortPlan>(
      rustCbor.encodeToByteArray(request),
      "sort",
      nativeCall,
    )
  val plan = checkNotNull(envelope.payload) { "Missing Download Catalog sort payload" }
  val expected = request.entries.map { it.galleryId }
  require(plan.galleryIds.size == expected.size && plan.galleryIds.toSet() == expected.toSet()) {
    "Download Catalog sort result is not a complete permutation"
  }
  return plan
}

/** Calls Rust once and validates a stable subset of the request identifiers. */
internal fun filterDownloadCatalog(
  request: DownloadFilterRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativeFilterDownloadCatalogV1,
): DownloadFilterPlan {
  val envelope =
    invokeDownloadCatalog<DownloadFilterPlan>(
      rustCbor.encodeToByteArray(request),
      "filter",
      nativeCall,
    )
  val plan = checkNotNull(envelope.payload) { "Missing Download Catalog filter payload" }
  val positions = request.entries.mapIndexed { index, entry -> entry.galleryId to index }.toMap()
  val resultPositions =
    plan.galleryIds.map { checkNotNull(positions[it]) { "Unknown filtered gallery" } }
  require(plan.galleryIds.distinct().size == plan.galleryIds.size) {
    "Download Catalog filter result contains duplicates"
  }
  require(resultPositions.zipWithNext().all { (left, right) -> left < right }) {
    "Download Catalog filter result changed input order"
  }
  return plan
}

private inline fun <reified T> invokeDownloadCatalog(
  request: ByteArray,
  operation: String,
  noinline nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): ApiEnvelope<T> {
  val envelope =
    invokeRustCbor<ApiEnvelope<T>>(
      request,
      operation = "Download Catalog $operation",
      nativeCall = nativeCall,
    )
  require(envelope.version == DOWNLOAD_CATALOG_API_VERSION) {
    "Unsupported Download Catalog API version"
  }
  require(envelope.module == DOWNLOAD_CATALOG_MODULE) {
    "Unexpected Download Catalog module"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return envelope
}

private external fun nativeSortDownloadCatalogV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private external fun nativeFilterDownloadCatalogV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val DOWNLOAD_CATALOG_API_VERSION = 1
internal const val DOWNLOAD_CATALOG_MODULE = "downloadCatalog"
