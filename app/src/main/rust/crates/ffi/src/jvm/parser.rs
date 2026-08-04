//! Response-parser JNI exports backed only by `eh_net` contracts.

use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{JNI_TRUE, jboolean, jint};
use jni_fn::jni_fn;

use eh_net::auth::profile::{
    parse_envelope as parse_profile_envelope, parse_url_envelope as parse_profile_url_envelope,
};
use eh_net::auth::sign_in::parse_envelope as parse_sign_in_envelope;
use eh_net::gallery::archive::{
    parse_list_envelope as parse_archive_list_envelope,
    parse_target_envelope as parse_archive_target_envelope,
};
use eh_net::gallery::comment_vote::parse_envelope as parse_gallery_comment_vote_envelope;
use eh_net::gallery::comments::parse_envelope as parse_gallery_comments_envelope;
use eh_net::gallery::detail::parse_envelope as parse_gallery_detail_envelope;
use eh_net::gallery::favorites::parse_envelope as parse_gallery_favorites_envelope;
use eh_net::gallery::list::parse_envelope as parse_gallery_list_envelope;
use eh_net::gallery::metadata::parse_envelope as parse_gallery_metadata_envelope;
use eh_net::gallery::preview::parse_envelope as parse_gallery_preview_envelope;
use eh_net::gallery::rate::parse_envelope as parse_gallery_rate_envelope;
use eh_net::gallery::tag_vote::parse_envelope as parse_gallery_tag_vote_envelope;
use eh_net::gallery::token::parse_envelope as parse_gallery_token_envelope;
use eh_net::home::parse_envelope as parse_image_limits_envelope;
use eh_net::news::parse_envelope as parse_news_envelope;
use eh_net::reader::mpv::parse_envelope as parse_mpv_tokens_envelope;
use eh_net::reader::page::parse_envelope as parse_reader_page_envelope;
use eh_net::torrent::parse_envelope as parse_torrent_envelope;
use eh_net::user_config::parse_envelope as parse_user_config_envelope;

use super::parse_text_in_place;

/// Parses remote Favorites HTML into the versioned workspace contract.
///
/// Android retains HTTP, paging state, local favorites, database enrichment, and
/// mutation execution. This response-only boundary never replays a mutation.
#[jni_fn("com.hippo.ehviewer.client.parser.FavoritesParserKt")]
pub fn parseFavV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_favorites_envelope)
}

/// Parses account image limits into the versioned Rust-first V1 envelope.
///
/// The unversioned `parseLimit` symbol remains the default-off rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.HomeParserKt")]
pub fn parseLimitV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_image_limits_envelope)
}

/// Parses News HTML event-pane markup into the versioned Rust-first V1 envelope.
///
/// Android retains HTTP, cookies, scheduling, and notification side effects. The
/// unversioned `parseEventPane` symbol remains the default-off rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.EventPaneParser")]
pub fn nativeParseNewsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_news_envelope)
}

/// Parses Gallery List HTML into the versioned Rust-first V1 envelope.
///
/// This symbol is deliberately separate from `parseGalleryInfoList`, which
/// remains the runtime rollback path with its legacy CBOR/error shape.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryListParserKt")]
pub fn parseGalleryInfoListV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_list_envelope)
}

/// Parses a sign-in response into the versioned authentication V1 envelope.
#[jni_fn("com.hippo.ehviewer.client.parser.SignInParser")]
pub fn parseSignInV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_sign_in_envelope)
}

/// Parses a rich Gallery Detail response into the versioned Rust-first V1 envelope.
///
/// The unversioned `nativeParse` symbol preserves the complete rollback contract.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParseV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_detail_envelope)
}

/// Parses gallery comments into the versioned Rust-first V1 envelope.
///
/// The unversioned `nativeParseComments` symbol remains the runtime rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParseCommentsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_comments_envelope)
}

/// Parses gallery previews into the versioned Rust-first V1 envelope.
///
/// The unversioned `nativeParsePreviews` symbol remains the runtime rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParsePreviewsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_preview_envelope)
}

/// Parses reader-page image metadata into the versioned Rust-first V1 envelope.
///
/// The original Kotlin parser remains the default-off runtime rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryPageParser")]
pub fn nativeParseReaderPageV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_reader_page_envelope)
}

/// Parses gallery MPV page tokens into the versioned Rust-first V1 envelope.
///
/// Android retains the request URL, HTTP session, token cache, page scheduling,
/// and preview fallback. Tokens are never copied into boundary errors.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryMultiPageViewerPTokenParser")]
pub fn nativeParseMpvTokensV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_mpv_tokens_envelope)
}

/// Parses one Gallery Token API response into the versioned Rust-first V1 envelope.
///
/// Android retains the request gallery identifier, existing token, page number,
/// HTTP session, and navigation. Response tokens never appear in boundary errors.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryTokenApiParser")]
pub fn nativeParseGalleryTokenV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_token_envelope)
}

/// Parses one Gallery Tag Vote API response into the versioned Rust-first V1 envelope.
///
/// Android retains credentials, gallery keys, submitted tag text, HTTP, and UI state.
#[jni_fn("com.hippo.ehviewer.client.parser.VoteTagParser")]
pub fn nativeParseTagVoteV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_tag_vote_envelope)
}

/// Parses one Gallery Rate API response into the versioned Rust-first V1 envelope.
///
/// Android retains credentials, gallery keys, rating rounding, HTTP, and UI state.
#[jni_fn("com.hippo.ehviewer.client.parser.RateGalleryParser")]
pub fn nativeParseGalleryRateV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_rate_envelope)
}

/// Parses one Gallery List `gdata` response into the versioned V1 envelope.
///
/// Android retains request batching, response-to-object matching, date presentation,
/// language derivation, and mutation. Rust receives only the response JSON.
#[jni_fn("com.hippo.ehviewer.client.parser.GalleryApiParser")]
pub fn nativeParseGalleryMetadataV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_gallery_metadata_envelope)
}

/// Parses one Gallery Comment Vote API response into the versioned V1 envelope.
///
/// Android retains credentials, request identifiers/vote, HTTP, refresh, and UI state.
#[jni_fn("com.hippo.ehviewer.client.parser.VoteCommentParser")]
pub fn nativeParseVoteCommentV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(
        &mut env,
        &buffer,
        limit,
        parse_gallery_comment_vote_envelope,
    )
}

/// Parses gallery torrent tables into a versioned list with validated info hashes.
///
/// Raw source URLs are intentionally absent from the V1 payload. The unversioned
/// `parseTorrent` symbol remains the default-off rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.TorrentParserKt")]
pub fn parseTorrentV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_torrent_envelope)
}

/// Parses user-config favorite-category names into the versioned V1 envelope.
///
/// The unversioned `parseFavCat` symbol remains the default-off rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.UserConfigParser")]
pub fn parseFavCatV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_user_config_envelope)
}

/// Parses archive choices and optional balances into the versioned V1 envelope.
///
/// This symbol remains separate from `parseArchives` so the unversioned parser
/// stays available as the default-off runtime rollback path.
#[jni_fn("com.hippo.ehviewer.client.parser.ArchiveParserKt")]
pub fn parseArchivesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
    include_funds: jboolean,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, |html| {
        parse_archive_list_envelope(html, include_funds == JNI_TRUE)
    })
}

/// Parses a validated final archive URL into the versioned V1 envelope.
///
/// Unsafe present links become typed Rust failures and never reach the legacy
/// string-concatenation parser when this default-off path is selected.
#[jni_fn("com.hippo.ehviewer.client.parser.ArchiveParserKt")]
pub fn parseArchiveUrlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_archive_target_envelope)
}

/// Extracts a profile URL into the versioned authentication V1 envelope.
#[jni_fn("com.hippo.ehviewer.client.parser.ProfileParser")]
pub fn parseProfileUrlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_profile_url_envelope)
}

/// Parses account profile HTML into the versioned authentication V1 envelope.
#[jni_fn("com.hippo.ehviewer.client.parser.ProfileParser")]
pub fn parseProfileV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    buffer: JByteBuffer<'_>,
    limit: jint,
) -> jint {
    parse_text_in_place(&mut env, &buffer, limit, parse_profile_envelope)
}
