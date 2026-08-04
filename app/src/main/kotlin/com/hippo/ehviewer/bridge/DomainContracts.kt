package com.hippo.ehviewer.bridge

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Stable error discriminants emitted by `eh_domain`. */
@Serializable
enum class RustErrorKind(internal val requiresMessage: Boolean) {
  @SerialName("noHits") NO_HITS(false),
  @SerialName("noWatched") NO_WATCHED(false),
  @SerialName("needLogin") NEED_LOGIN(false),
  @SerialName("noHathClient") NO_HATH_CLIENT(false),
  @SerialName("insufficientFunds") INSUFFICIENT_FUNDS(false),
  @SerialName("galleryUnavailable") GALLERY_UNAVAILABLE(true),
  @SerialName("ipBanned") IP_BANNED(true),
  @SerialName("error") ERROR(true),
}

/**
 * Versioned Rust business failure carried over the JNI/CBOR boundary.
 *
 * @property kind Stable wire discriminator owned by `eh_domain::contracts::RustError`.
 * @property message Diagnostic detail. It is required only for error kinds that carry text.
 */
@Serializable
data class RustError(val kind: RustErrorKind, val message: String? = null) {
  init {
    require(kind.requiresMessage == (message != null)) {
      "The message must be present exactly when the Rust error kind requires it"
    }
  }
}

/**
 * Versioned response from one Rust feature module.
 *
 * Exactly one of [payload] and [error] must be present. Unsupported [version] values are rejected
 * by the bridge adapter before the payload reaches UI code.
 */
@Serializable
data class ApiEnvelope<T>(
  val version: Int,
  val module: String,
  val payload: T? = null,
  val error: RustError? = null,
) {
  init {
    require((payload != null) != (error != null)) {
      "A Rust API envelope must contain exactly one of payload or error"
    }
  }

  /** Returns whether this envelope contains a success payload and no error. */
  fun isOk(): Boolean = payload != null && error == null
}

/** Identity confirmed by the V1 `eh_net::auth::sign_in` response parser. */
@Serializable data class SignInIdentity(val username: String)

/** Account presentation data emitted by the V1 `eh_net::auth::profile` parser. */
@Serializable data class AccountProfile(val displayName: String, val avatar: String? = null)

/**
 * Authentication and account capabilities for the active Rust session.
 *
 * This type mirrors `eh_domain::contracts::SessionState` field-for-field.
 */
@Serializable
data class SessionState(
  val loggedIn: Boolean,
  val username: String? = null,
  val ipBanned: Boolean,
  val hasHath: Boolean,
  val sufficientFunds: Boolean,
)

/** Catalog fields needed to render, favorite, and open one gallery result. */
@Serializable
data class GalleryItemSummary(
  @SerialName("gid") val galleryId: Long,
  val title: String,
  val token: String,
  @SerialName("titleJpn") val titleJapanese: String? = null,
  @SerialName("thumbKey") val thumbnailKey: String,
  val category: Int,
  val posted: String,
  val uploader: String? = null,
  val disowned: Boolean,
  val rating: Float,
  val rated: Boolean,
  @SerialName("simpleTags") val tags: List<String> = emptyList(),
  @SerialName("pages") val pageCount: Int,
  @SerialName("thumbWidth") val thumbnailWidth: Int,
  @SerialName("thumbHeight") val thumbnailHeight: Int,
  @SerialName("simpleLanguage") val language: String? = null,
  val favoriteSlot: Int,
  val favoriteName: String? = null,
  val favoriteNote: String? = null,
)

/** One cursor-based page of gallery catalog results. */
@Serializable
data class GalleryPage(
  @SerialName("prev") val previous: String? = null,
  val next: String? = null,
  @SerialName("galleryInfoList") val items: List<GalleryItemSummary> = emptyList(),
)

/** Opaque capability emitted by the V1 `eh_net::gallery::token` parser. */
@Serializable data class GalleryToken(val token: String)

/** Rendering strategy emitted by the V1 gallery-preview parser. */
@Serializable
enum class GalleryPreviewKind {
  /** A standalone preview image without sprite geometry. */
  @SerialName("v1") V1,

  /** A clipped region inside a preview sprite. */
  @SerialName("v2") V2,
}

/** Stable preview-thumbnail contract emitted by `eh_net::gallery::preview`. */
@Serializable
data class GalleryPreview(
  val kind: GalleryPreviewKind,
  val url: String,
  val position: Int,
  @SerialName("pToken") val pageToken: String,
  val offsetX: Int? = null,
  val clipWidth: Int? = null,
  val clipHeight: Int? = null,
)

/** Preview thumbnails and total image-page count for one gallery. */
@Serializable
data class GalleryPreviewPage(
  val previews: List<GalleryPreview> = emptyList(),
  val total: Int,
)

/** Stable gallery-comment contract emitted by `eh_net::gallery::comments`. */
@Serializable
data class GalleryComment(
  val id: Long,
  val score: Int,
  val editable: Boolean,
  @SerialName("voteUpAble") val canVoteUp: Boolean,
  @SerialName("voteUpEd") val votedUp: Boolean,
  @SerialName("voteDownAble") val canVoteDown: Boolean,
  @SerialName("voteDownEd") val votedDown: Boolean,
  val uploader: Boolean,
  val voteState: String? = null,
  @SerialName("time") val postedAt: Long,
  val user: String? = null,
  @SerialName("comment") val bodyHtml: String,
  @SerialName("lastEdited") val lastEditedAt: Long,
)

/** One parsed page of gallery comments. */
@Serializable
data class GalleryCommentsPage(
  val comments: List<GalleryComment> = emptyList(),
  val hasMore: Boolean,
)

/** One newer gallery version linked from a detail response. */
@Serializable
data class GalleryVersionSummary(
  @SerialName("gid") val galleryId: Long,
  val token: String,
  val title: String,
  val posted: String,
)

/** Stable remote strength of a gallery tag. */
@Serializable
enum class GalleryTagPower {
  @SerialName("solid") SOLID,
  @SerialName("active") ACTIVE,
  @SerialName("weak") WEAK,
}

/** Stable current-account vote state of a gallery tag. */
@Serializable
enum class GalleryTagVote {
  @SerialName("none") NONE,
  @SerialName("up") UP,
  @SerialName("down") DOWN,
}

/** One gallery tag in the versioned detail contract. */
@Serializable
data class GalleryTag(
  val text: String,
  val power: GalleryTagPower,
  val vote: GalleryTagVote,
)

/** Gallery tags grouped under one stable namespace string. */
@Serializable
data class GalleryTagGroup(
  val namespace: String,
  val tags: List<GalleryTag> = emptyList(),
)

/** Updated tag groups emitted by the V1 `eh_net::gallery::tag_vote` parser. */
@Serializable data class GalleryTags(val groups: List<GalleryTagGroup> = emptyList())

/** Updated score and vote count emitted by the V1 `eh_net::gallery::rate` parser. */
@Serializable data class GalleryRating(val rating: Float, val ratingCount: Int)

/** Source-ordered metadata emitted by the V1 `eh_net::gallery::metadata` parser. */
@Serializable data class GalleryMetadataBatch(val items: List<GalleryMetadataItem> = emptyList())

/**
 * Normalized metadata for one Gallery List entry.
 *
 * Android retains response-to-object matching, date presentation, language derivation, and
 * mutation. [posted] therefore remains a Unix timestamp in seconds.
 */
@Serializable
data class GalleryMetadataItem(
  @SerialName("gid") val galleryId: Long,
  val title: String,
  @SerialName("titleJpn") val titleJapanese: String,
  val category: Int,
  @SerialName("thumbKey") val thumbnailKey: String,
  val uploader: String? = null,
  val posted: Long,
  val rating: Float,
  @SerialName("simpleTags") val tags: List<String>,
  @SerialName("pages") val pageCount: Int,
)

/** Updated comment state emitted by the V1 `eh_net::gallery::comment_vote` parser. */
@Serializable data class GalleryCommentVote(val id: Long, val score: Int, val vote: Int)

/** Remote favorite categories and gallery page emitted by `eh_net::gallery::favorites`. */
@Serializable
data class GalleryFavoritesPage(
  val categories: List<String>,
  val counts: List<Int>,
  val page: GalleryPage,
)

/** Rich metadata required by the existing Android gallery-detail screen. */
@Serializable
data class GalleryDetail(
  val galleryInfo: GalleryItemSummary,
  val apiUid: Long,
  val apiKey: String? = null,
  val torrentCount: Int,
  val parent: String? = null,
  val newerVersions: List<GalleryVersionSummary> = emptyList(),
  val visible: String? = null,
  val language: String? = null,
  val size: String? = null,
  val favoriteCount: Int,
  val ratingCount: Int,
)

/** Complete V1 payload parsed from one Gallery Detail response. */
@Serializable
data class GalleryDetailPage(
  val detail: GalleryDetail,
  val tagGroups: List<GalleryTagGroup> = emptyList(),
  val comments: GalleryCommentsPage,
  val previews: GalleryPreviewPage,
  val eventHtml: String? = null,
)

/** Optional event-pane markup emitted by the V1 `eh_net::news` parser. */
@Serializable data class NewsPage(val eventHtml: String? = null)

/** Image-source metadata emitted by the V1 `eh_net::reader::page` parser. */
@Serializable
data class ReaderPage(
  val imageUrl: String,
  val skipHathKey: String? = null,
  val originalImageUrl: String? = null,
  val showKey: String? = null,
)

/** Page-index-ordered tokens emitted by the V1 `eh_net::reader::mpv` parser. */
@Serializable data class MpvTokens(val tokens: List<String>)

/** Image-request quota state emitted by `eh_net::home`. */
@Serializable data class ImageLimits(val current: Int, val maximum: Int, val resetCost: Int)

/** Favorite-category names emitted by `eh_net::user_config`. */
@Serializable data class FavoriteCategories(val names: List<String>)

/** One validated torrent-list entry emitted by `eh_net::torrent`. */
@Serializable
data class TorrentEntry(
  val outdated: Boolean,
  val posted: String,
  val size: String,
  val seeds: Int,
  val peers: Int,
  val downloads: Int,
  val uploader: String,
  val infoHash: String,
  val name: String,
)

/** Validated torrents from one gallery torrent-list response. */
@Serializable data class TorrentListPage(val torrents: List<TorrentEntry>)

/** One archive-download choice emitted by `eh_net::gallery::archive`. */
@Serializable
data class ArchiveOption(
  @SerialName("res") val resolution: String,
  val name: String,
  val size: String,
  val cost: String,
  @SerialName("isHath") val isHath: Boolean,
)

/** Account balances displayed with archive-download choices. */
@Serializable data class ArchiveFunds(val gp: Int, val credit: Int)

/** Archive choices plus balances when the archive page owns the funds source. */
@Serializable
data class ArchiveListPage(
  @SerialName("archiveList") val options: List<ArchiveOption> = emptyList(),
  val funds: ArchiveFunds? = null,
)

/** Validated final archive URL, or a successful pending server response. */
@Serializable data class ArchiveDownloadTarget(val url: String? = null)

/** Serializable progress state for one Android-owned download task. */
@Serializable
data class DownloadTask(
  val taskId: String,
  val galleryId: String,
  val path: String,
  val downloadedPages: Int,
  val totalPages: Int,
  val completed: Boolean,
)
