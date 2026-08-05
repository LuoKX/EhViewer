//! Versioned response, error, and data-transfer contracts.
//!
//! Field names, optionality, integer widths, and [`RustError`] discriminants are
//! serialized protocol data. DTO fields use `camelCase` to match the Android
//! contract. Adapters may wrap these types, but must not reinterpret or silently
//! alter that wire representation.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

/// A business failure returned by the Rust-first core.
///
/// The externally tagged `kind` names use `camelCase`. Variants carrying text
/// serialize that value in a `message` field. Transport and JNI failures do not
/// belong in this type; the boundary crate maps those failures separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum RustError {
    /// A catalog or search query completed successfully but matched no galleries.
    NoHits,
    /// The watched-only catalog contains no matching galleries.
    NoWatched,
    /// The current session is absent, expired, or otherwise requires login.
    NeedLogin,
    /// The requested operation requires a configured H@H client.
    NoHathClient,
    /// The account lacks the funds required by the requested operation.
    InsufficientFunds,
    /// The gallery cannot be opened; the value contains the server-facing reason.
    GalleryUnavailable(String),
    /// Requests are blocked for the current IP address; the value contains details.
    IpBanned(String),
    /// An unclassified domain failure; the value contains a diagnostic message.
    Error(String),
}

impl fmt::Display for RustError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHits => formatter.write_str("NoHits"),
            Self::NoWatched => formatter.write_str("NoWatched"),
            Self::NeedLogin => formatter.write_str("NeedLogin"),
            Self::NoHathClient => formatter.write_str("NoHathClient"),
            Self::InsufficientFunds => formatter.write_str("InsufficientFunds"),
            Self::GalleryUnavailable(message) => {
                write!(formatter, "GalleryUnavailable: {message}")
            }
            Self::IpBanned(message) => write!(formatter, "IpBanned: {message}"),
            Self::Error(message) => write!(formatter, "Error: {message}"),
        }
    }
}

impl std::error::Error for RustError {}

/// A versioned response produced by one Rust-first feature module.
///
/// Constructors guarantee that exactly one of [`Self::payload`] and
/// [`Self::error`] is populated. Fields remain public because this structure is
/// a serialized DTO; decoders must reject unsupported versions at their
/// boundary before consuming the payload. The structure is non-exhaustive so
/// downstream crates must use the constructors instead of creating invalid
/// combinations of public wire fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct ApiEnvelope<T> {
    /// The positive protocol version understood by the producing module.
    pub version: u8,
    /// The stable feature-module identifier, such as `auth` or `gallery`.
    pub module: String,
    /// The success value, or `None` when [`Self::error`] is populated.
    pub payload: Option<T>,
    /// The typed business failure, or `None` when [`Self::payload`] is populated.
    pub error: Option<RustError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiEnvelopeWire<T> {
    version: u8,
    module: String,
    payload: Option<T>,
    error: Option<RustError>,
}

impl<'de, T> Deserialize<'de> for ApiEnvelope<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ApiEnvelopeWire::deserialize(deserializer)?;
        let envelope = Self {
            version: wire.version,
            module: wire.module,
            payload: wire.payload,
            error: wire.error,
        };

        if envelope.is_valid() {
            Ok(envelope)
        } else {
            Err(D::Error::custom(
                "an API envelope must contain exactly one of payload or error",
            ))
        }
    }
}

impl<T> ApiEnvelope<T> {
    /// Creates a successful response containing `payload`.
    ///
    /// The caller owns version negotiation and must provide a stable module
    /// identifier. This constructor does not perform transport serialization.
    #[must_use]
    pub fn ok(version: u8, module: impl Into<String>, payload: T) -> Self {
        Self {
            version,
            module: module.into(),
            payload: Some(payload),
            error: None,
        }
    }

    /// Creates a failed response containing a typed domain `error`.
    ///
    /// The caller owns version negotiation and must provide a stable module
    /// identifier. No partial payload is retained in an error response.
    #[must_use]
    pub fn err(version: u8, module: impl Into<String>, error: RustError) -> Self {
        Self {
            version,
            module: module.into(),
            payload: None,
            error: Some(error),
        }
    }

    /// Returns `true` when the response contains a payload and no domain error.
    ///
    /// Invalid states containing neither field or both fields are not successful.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.payload.is_some() && self.error.is_none()
    }

    /// Returns `true` when exactly one of the payload and error fields is populated.
    ///
    /// The constructors always produce a valid state, and deserialization rejects
    /// wire values for which this predicate is false.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.payload.is_some() != self.error.is_some()
    }
}

/// Identity confirmed by a successful authentication response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInIdentity {
    /// The forum username reported by the remote sign-in endpoint.
    pub username: String,
}

/// Account presentation data loaded after authentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    /// The display name shown in account UI.
    pub display_name: String,
    /// The absolute avatar URL, or `None` when the account has no avatar.
    pub avatar: Option<String>,
}

/// Authentication and account-capability state for the active session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the compatibility DTO must retain its established serialized boolean fields"
)]
pub struct SessionState {
    /// Whether authenticated requests may use the active session.
    pub logged_in: bool,
    /// The display username when known; `None` for anonymous or unknown users.
    pub username: Option<String>,
    /// Whether the remote service reports an IP-address ban.
    pub ip_banned: bool,
    /// Whether a H@H client is available to the account.
    pub has_hath: bool,
    /// Whether the account currently has sufficient funds for gated operations.
    pub sufficient_funds: bool,
}

/// The catalog fields needed to render, favorite, and open one gallery result.
///
/// Explicit `serde` names preserve the existing Android CBOR contract while
/// Rust field names remain descriptive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItemSummary {
    /// The numeric gallery identifier supplied by the remote service.
    #[serde(rename = "gid")]
    pub gallery_id: i64,
    /// The display title selected for the gallery.
    pub title: String,
    /// The gallery access token required to open the detail page.
    pub token: String,
    /// The Japanese title when the source exposes one.
    #[serde(rename = "titleJpn")]
    pub title_japanese: Option<String>,
    /// The normalized thumbnail cache key.
    #[serde(rename = "thumbKey")]
    pub thumbnail_key: String,
    /// The existing Android category bit value.
    pub category: i32,
    /// The source-provided posting time text.
    pub posted: String,
    /// The uploader display name, or `None` for absent/disowned uploaders.
    pub uploader: Option<String>,
    /// Whether the gallery is marked as disowned.
    pub disowned: bool,
    /// The parsed zero-to-five rating, or a negative value when unavailable.
    pub rating: f32,
    /// Whether the current account has rated the gallery.
    pub rated: bool,
    /// Simplified tag labels in source order.
    #[serde(rename = "simpleTags")]
    pub tags: Vec<String>,
    /// Total image pages reported by the listing.
    #[serde(rename = "pages")]
    pub page_count: i32,
    /// Thumbnail width in pixels, or zero when absent.
    #[serde(rename = "thumbWidth")]
    pub thumbnail_width: i32,
    /// Thumbnail height in pixels, or zero when absent.
    #[serde(rename = "thumbHeight")]
    pub thumbnail_height: i32,
    /// Simplified language label when already derived.
    #[serde(rename = "simpleLanguage")]
    pub language: Option<String>,
    /// Existing Android favorite slot, with `-2` meaning not favorited.
    pub favorite_slot: i32,
    /// Favorite category display name when favorited.
    pub favorite_name: Option<String>,
    /// User-authored favorite note when present.
    pub favorite_note: Option<String>,
}

/// One cursor-based page of catalog results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPage {
    /// Cursor for the previous page, or `None` at the beginning.
    #[serde(rename = "prev")]
    pub previous: Option<String>,
    /// Cursor for the next page, or `None` at the end.
    pub next: Option<String>,
    /// Gallery summaries in source order; empty means the page has no matches.
    #[serde(rename = "galleryInfoList")]
    pub items: Vec<GalleryItemSummary>,
}

/// The first gallery access token returned by the remote `gtoken` API.
///
/// The source response also carries a gallery identifier. The network parser
/// validates that it is a signed 64-bit integer but deliberately does not expose
/// or compare it because the existing Android parser ignores that value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryToken {
    /// Opaque gallery capability forwarded to Android navigation unchanged.
    ///
    /// Empty strings and values outside the usual token format remain valid for
    /// compatibility with the source parser. Callers must never log this value.
    pub token: String,
}

/// Rendering strategy used by one gallery-preview thumbnail.
///
/// `V1` previews reference a standalone image. `V2` previews reference a
/// horizontal clip inside a sprite image and therefore require geometry fields
/// on [`GalleryPreview`]. The lowercase discriminants are part of the V1 wire
/// protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GalleryPreviewKind {
    /// A standalone preview image without sprite geometry.
    #[serde(rename = "v1")]
    V1,
    /// A clipped region inside a preview sprite.
    #[serde(rename = "v2")]
    V2,
}

/// One preview thumbnail returned by the gallery-preview parser.
///
/// For [`GalleryPreviewKind::V1`], the three geometry fields are `None`. For
/// [`GalleryPreviewKind::V2`], all three are populated. Android validates this
/// invariant before adapting the contract to its sealed preview model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPreview {
    /// Determines whether the URL is standalone or contains a sprite clip.
    pub kind: GalleryPreviewKind,
    /// Absolute preview image or sprite URL.
    pub url: String,
    /// Zero-based image position within the gallery; bridge adapters require it
    /// to be smaller than [`GalleryPreviewPage::total`].
    pub position: i32,
    /// Per-page access token, or an empty string for hash-based preview links.
    #[serde(rename = "pToken")]
    pub page_token: String,
    /// Horizontal sprite offset in pixels for V2 previews.
    pub offset_x: Option<i32>,
    /// Sprite clip width in pixels for V2 previews.
    pub clip_width: Option<i32>,
    /// Sprite clip height in pixels for V2 previews.
    pub clip_height: Option<i32>,
}

impl GalleryPreview {
    /// Creates a standalone V1 preview.
    #[must_use]
    pub fn v1(url: impl Into<String>, position: i32, page_token: impl Into<String>) -> Self {
        Self {
            kind: GalleryPreviewKind::V1,
            url: url.into(),
            position,
            page_token: page_token.into(),
            offset_x: None,
            clip_width: None,
            clip_height: None,
        }
    }

    /// Creates a sprite-backed V2 preview with complete clipping geometry.
    #[must_use]
    pub fn v2(
        url: impl Into<String>,
        position: i32,
        page_token: impl Into<String>,
        offset_x: i32,
        clip_width: i32,
        clip_height: i32,
    ) -> Self {
        Self {
            kind: GalleryPreviewKind::V2,
            url: url.into(),
            position,
            page_token: page_token.into(),
            offset_x: Some(offset_x),
            clip_width: Some(clip_width),
            clip_height: Some(clip_height),
        }
    }
}

/// A gallery's preview thumbnails and total image-page count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPreviewPage {
    /// Non-empty preview thumbnails in source order.
    pub previews: Vec<GalleryPreview>,
    /// Total image pages reported by the gallery detail table.
    pub total: i32,
}

/// One gallery comment and its current account-specific interaction state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the compatibility DTO preserves independent remote interaction flags"
)]
pub struct GalleryComment {
    /// Stable remote comment identifier; zero identifies the gallery uploader.
    pub id: i64,
    /// Current vote score reported by the detail page.
    pub score: i32,
    /// Whether the current account may edit this comment.
    pub editable: bool,
    /// Whether the current account may cast an up-vote.
    #[serde(rename = "voteUpAble")]
    pub can_vote_up: bool,
    /// Whether the current account has already cast an up-vote.
    #[serde(rename = "voteUpEd")]
    pub voted_up: bool,
    /// Whether the current account may cast a down-vote.
    #[serde(rename = "voteDownAble")]
    pub can_vote_down: bool,
    /// Whether the current account has already cast a down-vote.
    #[serde(rename = "voteDownEd")]
    pub voted_down: bool,
    /// Whether the comment belongs to the gallery uploader.
    pub uploader: bool,
    /// Source-provided vote-state text when present.
    pub vote_state: Option<String>,
    /// UTC posting timestamp in milliseconds.
    #[serde(rename = "time")]
    pub posted_at: i64,
    /// Comment author display name when present.
    pub user: Option<String>,
    /// Sanitized comment body HTML expected by the existing Android renderer.
    #[serde(rename = "comment")]
    pub body_html: String,
    /// UTC last-edit timestamp in milliseconds, or zero when absent.
    #[serde(rename = "lastEdited")]
    pub last_edited_at: i64,
}

/// One response page of gallery comments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryCommentsPage {
    /// Comments in source order; the list may be empty.
    pub comments: Vec<GalleryComment>,
    /// Whether the remote page exposes a control for loading older comments.
    pub has_more: bool,
}

/// One newer version linked from a gallery detail response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryVersionSummary {
    /// Stable numeric gallery identifier.
    #[serde(rename = "gid")]
    pub gallery_id: i64,
    /// Gallery access token.
    pub token: String,
    /// Display title reported by the version link.
    pub title: String,
    /// Source-provided posting time text.
    pub posted: String,
}

/// Strength assigned to a gallery tag by the remote service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryTagPower {
    /// A normal-strength tag.
    Solid,
    /// A positively reinforced tag.
    Active,
    /// A weak tag.
    Weak,
}

/// Current account vote state for a gallery tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryTagVote {
    /// The current account has not voted on the tag.
    None,
    /// The current account voted the tag up.
    Up,
    /// The current account voted the tag down.
    Down,
}

/// One parsed gallery tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTag {
    /// Display text with aliases removed.
    pub text: String,
    /// Remote tag strength.
    pub power: GalleryTagPower,
    /// Current account vote state.
    pub vote: GalleryTagVote,
}

/// Tags grouped under one source namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTagGroup {
    /// Stable namespace value, such as `artist` or `language`.
    pub namespace: String,
    /// Tags in source order.
    pub tags: Vec<GalleryTag>,
}

/// Gallery tag groups returned after one remote tag-vote operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTags {
    /// Updated groups in source order, including groups with no parsed tags.
    pub groups: Vec<GalleryTagGroup>,
}

/// Updated aggregate rating returned after one remote gallery-rating operation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryRating {
    /// Average rating represented with the existing Android `Float` precision.
    pub rating: f32,
    /// Source-provided signed rating count without additional range policy.
    pub rating_count: i32,
}

/// Normalized metadata returned for one batch of Gallery List entries.
///
/// Items remain in remote response order. Android owns request batching, matching
/// each [`GalleryMetadataItem::gallery_id`] to its first mutable list entry, date
/// presentation, and language derivation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryMetadataBatch {
    /// Metadata records in source order, including duplicate gallery identifiers.
    pub items: Vec<GalleryMetadataItem>,
}

/// Platform-neutral fields parsed from one remote `gdata` metadata record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryMetadataItem {
    /// Signed gallery identifier used only for Android-side list matching.
    #[serde(rename = "gid")]
    pub gallery_id: i64,
    /// Primary title after legacy-compatible known-entity decoding.
    pub title: String,
    /// Japanese title after legacy-compatible known-entity decoding.
    #[serde(rename = "titleJpn")]
    pub title_japanese: String,
    /// Existing Android category bit, or the unknown bit for unmatched labels.
    pub category: i32,
    /// Thumbnail key with a recognized gallery host and optional `t/` removed.
    #[serde(rename = "thumbKey")]
    pub thumbnail_key: String,
    /// Optional uploader after legacy-compatible known-entity decoding.
    pub uploader: Option<String>,
    /// Source Unix timestamp in seconds; Android owns display formatting.
    pub posted: i64,
    /// Source rating narrowed to the existing Android `Float` precision.
    pub rating: f32,
    /// Simple tags copied unchanged and in source order.
    #[serde(rename = "simpleTags")]
    pub tags: Vec<String>,
    /// Source-provided signed page count without additional range policy.
    #[serde(rename = "pages")]
    pub page_count: i32,
}

/// Updated comment state returned after one remote comment-vote operation.
///
/// The fields intentionally preserve their complete signed wire ranges. Android
/// currently uses only [`Self::vote`], but retaining the identifier and score
/// keeps the V1 response compatible with the existing Kotlin DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryCommentVote {
    /// Source-provided comment identifier without request-identity validation.
    pub id: i64,
    /// Updated signed comment score without additional range policy.
    pub score: i32,
    /// Source-provided signed vote state; zero and values outside `-1..=1` remain valid.
    pub vote: i32,
}

/// Favorite categories and one remote gallery page parsed from Favorites HTML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFavoritesPage {
    /// Category names in their source order after the legacy ordinal-10 skip.
    pub categories: Vec<String>,
    /// Signed category counts paired one-to-one with [`Self::categories`].
    pub counts: Vec<i32>,
    /// Cursor and gallery entries parsed through the shared Gallery List grammar.
    pub page: GalleryPage,
}

/// Rich metadata required by the existing Android gallery-detail screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryDetail {
    /// Shared gallery fields consumed by lists, cache, database, and reader links.
    pub gallery_info: GalleryItemSummary,
    /// Account API user identifier; `-1` represents an anonymous API context.
    pub api_uid: i64,
    /// Account API key when the source exposes one.
    pub api_key: Option<String>,
    /// Number of torrent options reported by the gallery.
    pub torrent_count: i32,
    /// Parent gallery URL when present.
    pub parent: Option<String>,
    /// Newer gallery versions in source order.
    pub newer_versions: Vec<GalleryVersionSummary>,
    /// Source-provided visibility label.
    pub visible: Option<String>,
    /// Source-provided language label before Android derives its simple form.
    pub language: Option<String>,
    /// Source-provided file-size label.
    pub size: Option<String>,
    /// Number of times the gallery has been favorited.
    pub favorite_count: i32,
    /// Number of submitted gallery ratings.
    pub rating_count: i32,
}

/// Complete V1 result of parsing one gallery-detail response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryDetailPage {
    /// Rich gallery metadata.
    pub detail: GalleryDetail,
    /// Structured tag groups.
    pub tag_groups: Vec<GalleryTagGroup>,
    /// Parsed comments and pagination state.
    pub comments: GalleryCommentsPage,
    /// Parsed preview thumbnails and total-page count.
    pub previews: GalleryPreviewPage,
    /// Optional event-pane HTML consumed by Android notifications.
    pub event_html: Option<String>,
}

/// Optional event-pane markup parsed from the remote News response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsPage {
    /// Inner HTML of the source `#eventpane`, or `None` when the pane is absent.
    ///
    /// An empty source pane remains `Some("")`; Android owns notification and
    /// scheduling behavior and must not reinterpret that value in the wire layer.
    pub event_html: Option<String>,
}

/// Image-source metadata parsed from one reader page response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderPage {
    /// Resampled image URL used for the normal reader request.
    pub image_url: String,
    /// H@H bypass key appended when retrying a failed image request.
    pub skip_hath_key: Option<String>,
    /// Original-resolution image URL when the source exposes one.
    pub original_image_url: Option<String>,
    /// API show-page key reused by subsequent reader-page requests.
    pub show_key: Option<String>,
}

/// Page tokens parsed from one gallery multi-page-viewer response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvTokens {
    /// Tokens in page-index order, including duplicates or empty source values.
    pub tokens: Vec<String>,
}

/// Image-request quota state displayed by the Android account dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageLimits {
    /// Number of image requests currently charged to the account.
    pub current: i32,
    /// Quota ceiling; `-1` means restricted, `0` unrestricted, and positive values are limits.
    pub maximum: i32,
    /// Gallery-Point cost of resetting the quota, or zero when reset is unavailable.
    pub reset_cost: i32,
}

/// Successful completion marker for an image-limit reset response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageLimitsReset {
    /// Always true for a successful response; insufficient funds is a typed error.
    pub accepted: bool,
}

/// Raw torrent key captured from the account home page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentKey {
    /// Source token captured without trimming or normalization.
    pub key: String,
}

/// Favorite-category names parsed from the remote user-configuration page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteCategories {
    /// Successfully matched and entity-decoded names in source order.
    pub names: Vec<String>,
}

/// One torrent-list entry rendered by the Android gallery dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentEntry {
    /// Whether the source marks this torrent as outdated.
    pub outdated: bool,
    /// Source-provided posting date without the time component.
    pub posted: String,
    /// Source-provided IEC size text.
    pub size: String,
    /// Current seed count.
    pub seeds: i32,
    /// Current peer count.
    pub peers: i32,
    /// Source-provided completed-download count.
    pub downloads: i32,
    /// Source-provided uploader label.
    pub uploader: String,
    /// Canonical lowercase 40-hex `BitTorrent` info hash.
    pub info_hash: String,
    /// Entity-decoded torrent display name.
    pub name: String,
}

/// Validated torrents parsed from one gallery torrent-list response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentListPage {
    /// Valid entries in source order; expunged or malformed entries are omitted.
    pub torrents: Vec<TorrentEntry>,
}

/// One archive-download choice rendered by the Android archive dialog.
///
/// The explicit `res` and `isHath` wire names preserve the existing Android
/// model while Rust uses descriptive field names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveOption {
    /// Remote resolution code, such as `org`, `1280`, or `1920`.
    #[serde(rename = "res")]
    pub resolution: String,
    /// Human-readable option label; direct downloads use an empty string.
    pub name: String,
    /// Source-provided estimated archive size.
    pub size: String,
    /// Source-provided cost with grouping commas removed for direct downloads.
    pub cost: String,
    /// Whether selecting this option submits an H@H download request.
    #[serde(rename = "isHath")]
    pub is_hath: bool,
}

/// Account balances displayed alongside archive-download choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveFunds {
    /// Gallery Points expressed in thousands, matching the existing Android UI.
    pub gp: i32,
    /// Credit balance expressed in whole credits.
    pub credit: i32,
}

/// Parsed archive-download choices and optional balances from one response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveListPage {
    /// Available choices in source order, with direct options before H@H options.
    #[serde(rename = "archiveList")]
    pub options: Vec<ArchiveOption>,
    /// Parsed balances, or `None` when Android supplied balances separately.
    pub funds: Option<ArchiveFunds>,
}

/// A validated final archive URL, or a successful pending response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveDownloadTarget {
    /// Absolute HTTPS download URL, or `None` while the server is preparing it.
    pub url: Option<String>,
}

/// Serializable progress state for one download task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTask {
    /// The application-assigned stable task identifier.
    pub task_id: String,
    /// The remote gallery identifier associated with the task.
    pub gallery_id: String,
    /// The Android-owned destination path represented as an opaque string.
    pub path: String,
    /// The number of pages persisted successfully so far.
    pub downloaded_pages: i32,
    /// The total number of pages expected for the task.
    pub total_pages: i32,
    /// Whether the task has reached its successful terminal state.
    pub completed: bool,
}

#[cfg(test)]
mod tests {
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::{Value, json};

    use super::{
        AccountProfile, ApiEnvelope, ArchiveDownloadTarget, ArchiveFunds, ArchiveListPage,
        ArchiveOption, DownloadTask, FavoriteCategories, GalleryComment, GalleryCommentVote,
        GalleryCommentsPage, GalleryDetail, GalleryDetailPage, GalleryFavoritesPage,
        GalleryItemSummary, GalleryMetadataBatch, GalleryMetadataItem, GalleryPage, GalleryPreview,
        GalleryPreviewKind, GalleryPreviewPage, GalleryRating, GalleryTag, GalleryTagGroup,
        GalleryTagPower, GalleryTagVote, GalleryTags, GalleryToken, GalleryVersionSummary,
        ImageLimits, MpvTokens, NewsPage, ReaderPage, RustError, SessionState, SignInIdentity,
        TorrentEntry, TorrentListPage,
    };

    fn assert_cbor_round_trip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let bytes = serde_cbor::to_vec(value).expect("test DTO should serialize to CBOR");
        let decoded =
            serde_cbor::from_slice(&bytes).expect("serialized test DTO should decode from CBOR");
        assert_eq!(value, &decoded);
    }

    fn complete_gallery_item() -> GalleryItemSummary {
        GalleryItemSummary {
            gallery_id: 101,
            title: "Complete".to_owned(),
            token: "token".to_owned(),
            title_japanese: Some("Japanese".to_owned()),
            thumbnail_key: "key-1".to_owned(),
            category: 4,
            posted: "2026-08-02".to_owned(),
            uploader: Some("uploader".to_owned()),
            disowned: false,
            rating: 4.5,
            rated: true,
            tags: vec!["language:english".to_owned()],
            page_count: 20,
            thumbnail_width: 120,
            thumbnail_height: 180,
            language: Some("English".to_owned()),
            favorite_slot: 0,
            favorite_name: Some("Favorites".to_owned()),
            favorite_note: Some("note".to_owned()),
        }
    }

    fn minimal_gallery_item() -> GalleryItemSummary {
        GalleryItemSummary {
            gallery_id: 102,
            title: "Minimal".to_owned(),
            token: "other-token".to_owned(),
            title_japanese: None,
            thumbnail_key: "key-2".to_owned(),
            category: 0x800,
            posted: String::new(),
            uploader: None,
            disowned: true,
            rating: -1.0,
            rated: false,
            tags: Vec::new(),
            page_count: 0,
            thumbnail_width: 0,
            thumbnail_height: 0,
            language: None,
            favorite_slot: -2,
            favorite_name: None,
            favorite_note: None,
        }
    }

    #[test]
    fn error_display_covers_every_variant() {
        let cases = [
            (RustError::NoHits, "NoHits"),
            (RustError::NoWatched, "NoWatched"),
            (RustError::NeedLogin, "NeedLogin"),
            (RustError::NoHathClient, "NoHathClient"),
            (RustError::InsufficientFunds, "InsufficientFunds"),
            (
                RustError::GalleryUnavailable("removed".to_owned()),
                "GalleryUnavailable: removed",
            ),
            (
                RustError::IpBanned("until tomorrow".to_owned()),
                "IpBanned: until tomorrow",
            ),
            (
                RustError::Error("unexpected".to_owned()),
                "Error: unexpected",
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn errors_keep_stable_serde_discriminants() {
        let cases = [
            (RustError::NoHits, json!({"kind": "noHits"})),
            (RustError::NoWatched, json!({"kind": "noWatched"})),
            (RustError::NeedLogin, json!({"kind": "needLogin"})),
            (RustError::NoHathClient, json!({"kind": "noHathClient"})),
            (
                RustError::InsufficientFunds,
                json!({"kind": "insufficientFunds"}),
            ),
            (
                RustError::GalleryUnavailable("removed".to_owned()),
                json!({"kind": "galleryUnavailable", "message": "removed"}),
            ),
            (
                RustError::IpBanned("until tomorrow".to_owned()),
                json!({"kind": "ipBanned", "message": "until tomorrow"}),
            ),
            (
                RustError::Error("unexpected".to_owned()),
                json!({"kind": "error", "message": "unexpected"}),
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(
                serde_json::to_value(&error).expect("error should serialize"),
                expected
            );
            assert_eq!(
                serde_json::from_value::<RustError>(expected)
                    .expect("stable error shape should deserialize"),
                error
            );
            assert_cbor_round_trip(&error);
        }
    }

    #[test]
    fn envelope_constructors_are_exclusive_and_keep_wire_fields() {
        let success = ApiEnvelope::ok(1, String::from("gallery"), vec!["first".to_owned()]);
        assert!(success.is_ok());
        assert!(success.is_valid());
        assert_eq!(
            success.payload.as_deref(),
            Some(["first".to_owned()].as_slice())
        );
        assert_eq!(success.error, None);
        assert_eq!(
            serde_json::to_value(&success).expect("success envelope should serialize"),
            json!({
                "version": 1,
                "module": "gallery",
                "payload": ["first"],
                "error": null,
            })
        );
        assert_cbor_round_trip(&success);

        let failure = ApiEnvelope::<Value>::err(1, "auth", RustError::NeedLogin);
        assert!(!failure.is_ok());
        assert!(failure.is_valid());
        assert_eq!(failure.payload, None);
        assert_eq!(failure.error, Some(RustError::NeedLogin));
        assert_eq!(
            serde_json::to_value(&failure).expect("error envelope should serialize"),
            json!({
                "version": 1,
                "module": "auth",
                "payload": null,
                "error": {"kind": "needLogin"},
            })
        );
        assert_cbor_round_trip(&failure);
    }

    #[test]
    fn envelope_rejects_invalid_wire_states() {
        let empty = json!({
            "version": 1,
            "module": "gallery",
            "payload": null,
            "error": null,
        });
        let empty_error = serde_json::from_value::<ApiEnvelope<Value>>(empty)
            .expect_err("an envelope without a result must be rejected");
        assert!(empty_error.to_string().contains("exactly one"));

        let conflicting = json!({
            "version": 1,
            "module": "gallery",
            "payload": {"page": 0},
            "error": {"kind": "needLogin"},
        });
        let conflicting_error = serde_json::from_value::<ApiEnvelope<Value>>(conflicting)
            .expect_err("an envelope with a payload and error must be rejected");
        assert!(conflicting_error.to_string().contains("exactly one"));

        let manually_empty = ApiEnvelope::<Value> {
            version: 1,
            module: "gallery".to_owned(),
            payload: None,
            error: None,
        };
        assert!(!manually_empty.is_valid());
        assert!(!manually_empty.is_ok());

        let manually_conflicting = ApiEnvelope {
            version: 1,
            module: "gallery".to_owned(),
            payload: Some(json!({"page": 0})),
            error: Some(RustError::NeedLogin),
        };
        assert!(!manually_conflicting.is_valid());
        assert!(!manually_conflicting.is_ok());
    }

    #[test]
    fn session_state_serde_preserves_optional_username() {
        let authenticated = SessionState {
            logged_in: true,
            username: Some("reader".to_owned()),
            ip_banned: false,
            has_hath: true,
            sufficient_funds: true,
        };
        assert_eq!(
            serde_json::to_value(&authenticated).expect("session should serialize"),
            json!({
                "loggedIn": true,
                "username": "reader",
                "ipBanned": false,
                "hasHath": true,
                "sufficientFunds": true,
            })
        );
        assert_cbor_round_trip(&authenticated);

        let anonymous = SessionState {
            logged_in: false,
            username: None,
            ip_banned: true,
            has_hath: false,
            sufficient_funds: false,
        };
        assert_eq!(
            serde_json::to_value(&anonymous).expect("anonymous session should serialize")["username"],
            Value::Null
        );
        assert_cbor_round_trip(&anonymous);
    }

    #[test]
    fn auth_contracts_preserve_identity_and_optional_avatar() {
        let identity = SignInIdentity {
            username: "reader".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(&identity).expect("identity should serialize"),
            json!({"username": "reader"})
        );
        assert_cbor_round_trip(&identity);

        let profile = AccountProfile {
            display_name: "Reader & Artist".to_owned(),
            avatar: Some("https://forums.e-hentai.org/avatar.png".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(&profile).expect("profile should serialize"),
            json!({
                "displayName": "Reader & Artist",
                "avatar": "https://forums.e-hentai.org/avatar.png",
            })
        );
        assert_cbor_round_trip(&profile);

        let without_avatar = AccountProfile {
            avatar: None,
            ..profile
        };
        assert_eq!(
            serde_json::to_value(&without_avatar).expect("profile should serialize")["avatar"],
            Value::Null
        );
        assert_cbor_round_trip(&without_avatar);
    }

    #[test]
    fn gallery_page_serde_preserves_item_order_and_optional_fields() {
        let page = GalleryPage {
            previous: Some("10-1".to_owned()),
            next: Some("20-1".to_owned()),
            items: vec![complete_gallery_item(), minimal_gallery_item()],
        };

        assert_eq!(
            serde_json::to_value(&page).expect("gallery page should serialize"),
            json!({
                "prev": "10-1",
                "next": "20-1",
                "galleryInfoList": [
                    {
                        "gid": 101,
                        "title": "Complete",
                        "token": "token",
                        "titleJpn": "Japanese",
                        "thumbKey": "key-1",
                        "category": 4,
                        "posted": "2026-08-02",
                        "uploader": "uploader",
                        "disowned": false,
                        "rating": 4.5,
                        "rated": true,
                        "simpleTags": ["language:english"],
                        "pages": 20,
                        "thumbWidth": 120,
                        "thumbHeight": 180,
                        "simpleLanguage": "English",
                        "favoriteSlot": 0,
                        "favoriteName": "Favorites",
                        "favoriteNote": "note",
                    },
                    {
                        "gid": 102,
                        "title": "Minimal",
                        "token": "other-token",
                        "titleJpn": null,
                        "thumbKey": "key-2",
                        "category": 2048,
                        "posted": "",
                        "uploader": null,
                        "disowned": true,
                        "rating": -1.0,
                        "rated": false,
                        "simpleTags": [],
                        "pages": 0,
                        "thumbWidth": 0,
                        "thumbHeight": 0,
                        "simpleLanguage": null,
                        "favoriteSlot": -2,
                        "favoriteName": null,
                        "favoriteNote": null,
                    },
                ],
            })
        );
        assert_cbor_round_trip(&page);

        let empty_page = GalleryPage {
            previous: None,
            next: None,
            items: Vec::new(),
        };
        assert_eq!(
            serde_json::to_value(&empty_page).expect("empty page should serialize")["galleryInfoList"],
            json!([])
        );
        assert_cbor_round_trip(&empty_page);
    }

    #[test]
    fn gallery_token_contract_preserves_opaque_and_empty_values() {
        for value in ["0439fa3666", "", "opaque-令牌"] {
            let token = GalleryToken {
                token: value.to_owned(),
            };
            assert_eq!(
                serde_json::to_value(&token).expect("gallery token should serialize"),
                json!({"token": value})
            );
            assert_cbor_round_trip(&token);
        }
    }

    #[test]
    fn gallery_preview_contract_preserves_kinds_geometry_and_tokens() {
        let page = GalleryPreviewPage {
            previews: vec![
                GalleryPreview::v1("https://img.example/one.jpg", 0, "abcdef1234"),
                GalleryPreview::v2("https://img.example/sprite.jpg", 1, "", 16, 80, 120),
            ],
            total: 12,
        };

        assert_eq!(
            serde_json::to_value(&page).expect("preview page should serialize"),
            json!({
                "previews": [
                    {
                        "kind": "v1",
                        "url": "https://img.example/one.jpg",
                        "position": 0,
                        "pToken": "abcdef1234",
                        "offsetX": null,
                        "clipWidth": null,
                        "clipHeight": null,
                    },
                    {
                        "kind": "v2",
                        "url": "https://img.example/sprite.jpg",
                        "position": 1,
                        "pToken": "",
                        "offsetX": 16,
                        "clipWidth": 80,
                        "clipHeight": 120,
                    },
                ],
                "total": 12,
            })
        );
        assert_eq!(page.previews[0].kind, GalleryPreviewKind::V1);
        assert_eq!(page.previews[1].kind, GalleryPreviewKind::V2);
        assert_cbor_round_trip(&page);
    }

    #[test]
    fn gallery_comments_contract_preserves_interaction_state_and_html() {
        let page = GalleryCommentsPage {
            comments: vec![GalleryComment {
                id: 123,
                score: -7,
                editable: true,
                can_vote_up: true,
                voted_up: true,
                can_vote_down: true,
                voted_down: false,
                uploader: false,
                vote_state: Some("Voted & noted".to_owned()),
                posted_at: 1_785_678_300_000,
                user: Some("Alice & Bob".to_owned()),
                body_html: "First <u>underlined</u>".to_owned(),
                last_edited_at: 1_785_767_400_000,
            }],
            has_more: true,
        };

        assert_eq!(
            serde_json::to_value(&page).expect("comments page should serialize"),
            json!({
                "comments": [{
                    "id": 123,
                    "score": -7,
                    "editable": true,
                    "voteUpAble": true,
                    "voteUpEd": true,
                    "voteDownAble": true,
                    "voteDownEd": false,
                    "uploader": false,
                    "voteState": "Voted & noted",
                    "time": 1_785_678_300_000_i64,
                    "user": "Alice & Bob",
                    "comment": "First <u>underlined</u>",
                    "lastEdited": 1_785_767_400_000_i64,
                }],
                "hasMore": true,
            })
        );
        assert_cbor_round_trip(&page);

        let empty = GalleryCommentsPage::default();
        assert_eq!(
            serde_json::to_value(&empty).expect("empty comments should serialize"),
            json!({"comments": [], "hasMore": false})
        );
        assert_cbor_round_trip(&empty);
    }

    #[test]
    fn gallery_tags_contract_preserves_group_tag_order_and_empty_values() {
        let tags = GalleryTags {
            groups: vec![
                GalleryTagGroup {
                    namespace: "artist".to_owned(),
                    tags: vec![GalleryTag {
                        text: String::new(),
                        power: GalleryTagPower::Active,
                        vote: GalleryTagVote::Up,
                    }],
                },
                GalleryTagGroup {
                    namespace: "language".to_owned(),
                    tags: Vec::new(),
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(&tags).expect("gallery tags should serialize"),
            json!({
                "groups": [
                    {
                        "namespace": "artist",
                        "tags": [{"text": "", "power": "active", "vote": "up"}],
                    },
                    {"namespace": "language", "tags": []},
                ],
            })
        );
        assert_cbor_round_trip(&tags);
    }

    #[test]
    fn gallery_rating_contract_preserves_float_precision_and_signed_count() {
        let rating = GalleryRating {
            rating: 4.75,
            rating_count: -1,
        };
        assert_eq!(
            serde_json::to_value(rating).expect("gallery rating should serialize"),
            json!({"rating": 4.75, "ratingCount": -1})
        );
        assert_cbor_round_trip(&rating);
    }

    #[test]
    fn gallery_metadata_contract_preserves_wire_names_order_and_boundaries() {
        let batch = GalleryMetadataBatch {
            items: vec![GalleryMetadataItem {
                gallery_id: i64::MIN,
                title: "Title & unknown".to_owned(),
                title_japanese: "日本語".to_owned(),
                category: 0x800,
                thumbnail_key: "thumb.jpg".to_owned(),
                uploader: None,
                posted: i64::MAX,
                rating: -0.0,
                tags: vec!["language:english".to_owned(), String::new()],
                page_count: i32::MIN,
            }],
        };
        assert_eq!(
            serde_json::to_value(&batch).expect("gallery metadata should serialize"),
            json!({
                "items": [{
                    "gid": i64::MIN,
                    "title": "Title & unknown",
                    "titleJpn": "日本語",
                    "category": 0x800,
                    "thumbKey": "thumb.jpg",
                    "uploader": null,
                    "posted": i64::MAX,
                    "rating": -0.0,
                    "simpleTags": ["language:english", ""],
                    "pages": i32::MIN,
                }]
            })
        );
        assert_cbor_round_trip(&batch);
    }

    #[test]
    fn gallery_comment_vote_contract_preserves_all_signed_fields() {
        let result = GalleryCommentVote {
            id: i64::MIN,
            score: i32::MAX,
            vote: 0,
        };
        assert_eq!(
            serde_json::to_value(result).expect("comment vote should serialize"),
            json!({"id": i64::MIN, "score": i32::MAX, "vote": 0})
        );
        assert_cbor_round_trip(&result);
    }

    #[test]
    fn gallery_favorites_contract_preserves_category_pairing_and_page() {
        let result = GalleryFavoritesPage {
            categories: vec!["Favorites & One".to_owned(), String::new()],
            counts: vec![i32::MIN, i32::MAX],
            page: GalleryPage {
                previous: Some("10-2".to_owned()),
                next: None,
                items: vec![complete_gallery_item()],
            },
        };
        assert_eq!(
            serde_json::to_value(&result).expect("favorites page should serialize"),
            json!({
                "categories": ["Favorites & One", ""],
                "counts": [i32::MIN, i32::MAX],
                "page": {
                    "prev": "10-2",
                    "next": null,
                    "galleryInfoList": [complete_gallery_item()],
                },
            })
        );
        assert_cbor_round_trip(&result);
    }

    #[test]
    fn gallery_detail_serde_preserves_nested_metadata_tags_and_event_html() {
        let detail = GalleryDetail {
            gallery_info: complete_gallery_item(),
            api_uid: -1,
            api_key: Some("api-key".to_owned()),
            torrent_count: 2,
            parent: Some("https://e-hentai.org/g/1/parenttoken/".to_owned()),
            newer_versions: vec![GalleryVersionSummary {
                gallery_id: 202,
                token: "new-token".to_owned(),
                title: "New version".to_owned(),
                posted: "2026-08-03".to_owned(),
            }],
            visible: Some("Yes".to_owned()),
            language: Some("English".to_owned()),
            size: Some("12 MiB".to_owned()),
            favorite_count: 3,
            rating_count: 10,
        };
        let page = GalleryDetailPage {
            detail,
            tag_groups: vec![GalleryTagGroup {
                namespace: "artist".to_owned(),
                tags: vec![GalleryTag {
                    text: "someone".to_owned(),
                    power: GalleryTagPower::Active,
                    vote: GalleryTagVote::Up,
                }],
            }],
            comments: GalleryCommentsPage::default(),
            previews: GalleryPreviewPage {
                previews: vec![GalleryPreview::v1(
                    "https://img.example/one.jpg",
                    0,
                    "token",
                )],
                total: 20,
            },
            event_html: Some("<strong>Event</strong>".to_owned()),
        };
        let encoded = serde_json::to_value(&page).expect("gallery detail should serialize");
        assert_eq!(encoded["detail"]["galleryInfo"]["gid"], json!(101));
        assert_eq!(encoded["detail"]["apiUid"], json!(-1));
        assert_eq!(encoded["detail"]["newerVersions"][0]["gid"], json!(202));
        assert_eq!(encoded["tagGroups"][0]["tags"][0]["power"], json!("active"));
        assert_eq!(encoded["tagGroups"][0]["tags"][0]["vote"], json!("up"));
        assert_eq!(encoded["eventHtml"], json!("<strong>Event</strong>"));
        assert_cbor_round_trip(&page);
    }

    #[test]
    fn news_page_contract_preserves_absent_empty_and_nested_event_html() {
        for event_html in [
            None,
            Some(String::new()),
            Some("<strong>Event &amp; reward</strong>".to_owned()),
        ] {
            let page = NewsPage {
                event_html: event_html.clone(),
            };
            assert_eq!(
                serde_json::to_value(&page).expect("news page should serialize"),
                json!({"eventHtml": event_html})
            );
            assert_cbor_round_trip(&page);
        }
    }

    #[test]
    fn reader_page_contract_preserves_optional_request_keys() {
        let complete = ReaderPage {
            image_url: "https://img.example/page.jpg".to_owned(),
            skip_hath_key: Some("skip".to_owned()),
            original_image_url: Some("https://img.example/fullimg/page.jpg".to_owned()),
            show_key: Some("show".to_owned()),
        };
        assert_eq!(
            serde_json::to_value(&complete).expect("reader page should serialize"),
            json!({
                "imageUrl": "https://img.example/page.jpg",
                "skipHathKey": "skip",
                "originalImageUrl": "https://img.example/fullimg/page.jpg",
                "showKey": "show",
            })
        );
        assert_cbor_round_trip(&complete);

        let minimal = ReaderPage {
            image_url: "https://img.example/minimal.jpg".to_owned(),
            skip_hath_key: None,
            original_image_url: None,
            show_key: None,
        };
        let encoded = serde_json::to_value(&minimal).expect("reader page should serialize");
        assert_eq!(encoded["skipHathKey"], Value::Null);
        assert_eq!(encoded["originalImageUrl"], Value::Null);
        assert_eq!(encoded["showKey"], Value::Null);
        assert_cbor_round_trip(&minimal);
    }

    #[test]
    fn mpv_tokens_contract_preserves_page_order_duplicates_and_empty_values() {
        let tokens = MpvTokens {
            tokens: vec![
                "0123456789".to_owned(),
                String::new(),
                "0123456789".to_owned(),
            ],
        };
        assert_eq!(
            serde_json::to_value(&tokens).expect("MPV tokens should serialize"),
            json!({"tokens": ["0123456789", "", "0123456789"]})
        );
        assert_cbor_round_trip(&tokens);

        let empty = MpvTokens { tokens: Vec::new() };
        assert_eq!(
            serde_json::to_value(&empty).expect("empty MPV tokens should serialize"),
            json!({"tokens": []})
        );
        assert_cbor_round_trip(&empty);
    }

    #[test]
    fn image_limits_contract_preserves_quota_sentinels_and_reset_cost() {
        let limited = ImageLimits {
            current: 1_234,
            maximum: 5_000,
            reset_cost: 12_345,
        };
        assert_eq!(
            serde_json::to_value(&limited).expect("image limits should serialize"),
            json!({"current": 1_234, "maximum": 5_000, "resetCost": 12_345})
        );
        assert_cbor_round_trip(&limited);

        for maximum in [-1, 0] {
            let sentinel = ImageLimits {
                current: 0,
                maximum,
                reset_cost: 0,
            };
            assert_cbor_round_trip(&sentinel);
            assert_eq!(
                serde_json::to_value(&sentinel).expect("image limits should serialize")["maximum"],
                json!(maximum)
            );
        }
    }

    #[test]
    fn favorite_categories_contract_preserves_source_order_and_partial_results() {
        let categories = FavoriteCategories {
            names: vec!["First".to_owned(), "Second & More".to_owned()],
        };
        assert_eq!(
            serde_json::to_value(&categories).expect("favorite categories should serialize"),
            json!({"names": ["First", "Second & More"]})
        );
        assert_cbor_round_trip(&categories);

        let empty = FavoriteCategories { names: Vec::new() };
        assert_eq!(
            serde_json::to_value(&empty).expect("empty categories should serialize"),
            json!({"names": []})
        );
        assert_cbor_round_trip(&empty);
    }

    #[test]
    fn torrent_contract_preserves_display_fields_and_canonical_info_hash() {
        let page = TorrentListPage {
            torrents: vec![TorrentEntry {
                outdated: true,
                posted: "2026-08-03".to_owned(),
                size: "1.5 MiB".to_owned(),
                seeds: 12,
                peers: 3,
                downloads: 45,
                uploader: "uploader".to_owned(),
                info_hash: "0123456789abcdef0123456789abcdef01234567".to_owned(),
                name: "Name & More".to_owned(),
            }],
        };
        assert_eq!(
            serde_json::to_value(&page).expect("torrent page should serialize"),
            json!({
                "torrents": [{
                    "outdated": true,
                    "posted": "2026-08-03",
                    "size": "1.5 MiB",
                    "seeds": 12,
                    "peers": 3,
                    "downloads": 45,
                    "uploader": "uploader",
                    "infoHash": "0123456789abcdef0123456789abcdef01234567",
                    "name": "Name & More",
                }]
            })
        );
        assert_cbor_round_trip(&page);
    }

    #[test]
    fn archive_contracts_preserve_legacy_wire_names_and_optional_values() {
        let page = ArchiveListPage {
            options: vec![
                ArchiveOption {
                    resolution: "org".to_owned(),
                    name: String::new(),
                    size: "15.47 GiB".to_owned(),
                    cost: "747708 GP".to_owned(),
                    is_hath: false,
                },
                ArchiveOption {
                    resolution: "1280".to_owned(),
                    name: "1280x".to_owned(),
                    size: "1.25 GiB".to_owned(),
                    cost: "Free".to_owned(),
                    is_hath: true,
                },
            ],
            funds: Some(ArchiveFunds {
                gp: 742_189,
                credit: 1_561_004_772,
            }),
        };

        assert_eq!(
            serde_json::to_value(&page).expect("archive page should serialize"),
            json!({
                "archiveList": [
                    {
                        "res": "org",
                        "name": "",
                        "size": "15.47 GiB",
                        "cost": "747708 GP",
                        "isHath": false,
                    },
                    {
                        "res": "1280",
                        "name": "1280x",
                        "size": "1.25 GiB",
                        "cost": "Free",
                        "isHath": true,
                    },
                ],
                "funds": {"gp": 742_189, "credit": 1_561_004_772},
            })
        );
        assert_cbor_round_trip(&page);

        let without_funds = ArchiveListPage {
            options: Vec::new(),
            funds: None,
        };
        assert_eq!(
            serde_json::to_value(&without_funds).expect("archive page should serialize"),
            json!({"archiveList": [], "funds": null})
        );
        assert_cbor_round_trip(&without_funds);

        let pending = ArchiveDownloadTarget { url: None };
        assert_eq!(
            serde_json::to_value(&pending).expect("archive target should serialize"),
            json!({"url": null})
        );
        assert_cbor_round_trip(&pending);
    }

    #[test]
    fn download_task_serde_preserves_progress_and_path() {
        let task = DownloadTask {
            task_id: "task-1".to_owned(),
            gallery_id: "301".to_owned(),
            path: "/android-owned/gallery".to_owned(),
            downloaded_pages: 4,
            total_pages: 10,
            completed: false,
        };
        assert_eq!(
            serde_json::to_value(&task).expect("download task should serialize"),
            json!({
                "taskId": "task-1",
                "galleryId": "301",
                "path": "/android-owned/gallery",
                "downloadedPages": 4,
                "totalPages": 10,
                "completed": false,
            })
        );
        assert_cbor_round_trip(&task);

        let completed = DownloadTask {
            downloaded_pages: 10,
            completed: true,
            ..task
        };
        assert_eq!(
            serde_json::from_value::<DownloadTask>(
                serde_json::to_value(&completed).expect("completed task should serialize")
            )
            .expect("completed task should deserialize"),
            completed
        );
        assert_cbor_round_trip(&completed);
    }
}
