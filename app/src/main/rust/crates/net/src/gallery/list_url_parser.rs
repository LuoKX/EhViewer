//! Gallery-list state interpretation from sanitized URL components.
//!
//! Android retains Ktor URL syntax parsing and the exact legacy host gate. It
//! sends only the first two meaningful path segments plus the first decoded value
//! of relevant query fields, so ignored path suffixes and unrelated parameters do
//! not cross JNI. This module owns mode/category/toplist classification, numeric
//! defaults, advanced-search flags, and the resulting platform-neutral builder
//! state. Input values are never logged or copied into errors.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

const MODE_NORMAL: i32 = 0;
const MODE_UPLOADER: i32 = 1;
const MODE_TAG: i32 = 2;
const MODE_IMAGE_SEARCH: i32 = 4;
const MODE_TOPLIST: i32 = 6;

const CATEGORY_MISC: i32 = 0x1;
const CATEGORY_DOUJINSHI: i32 = 0x2;
const CATEGORY_MANGA: i32 = 0x4;
const CATEGORY_ARTIST_CG: i32 = 0x8;
const CATEGORY_GAME_CG: i32 = 0x10;
const CATEGORY_IMAGE_SET: i32 = 0x20;
const CATEGORY_COSPLAY: i32 = 0x40;
const CATEGORY_ASIAN_PORN: i32 = 0x80;
const CATEGORY_NON_H: i32 = 0x100;
const CATEGORY_WESTERN: i32 = 0x200;
const CATEGORY_PRIVATE: i32 = 0x400;
const ALL_CATEGORIES: i32 = 0x3ff;

const ADVANCED_SEARCH_SH: i32 = 1;
const ADVANCED_SEARCH_STO: i32 = 2;
const ADVANCED_SEARCH_SFL: i32 = 4;
const ADVANCED_SEARCH_SFU: i32 = 8;
const ADVANCED_SEARCH_SFT: i32 = 16;

/// Current sanitized gallery-list URL parser protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android parser adapter.
pub const MODULE: &str = "galleryListUrlParser";

/// First decoded values of the URL query fields consumed by the legacy parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlParameters {
    /// `f_shash`; presence, including an empty value, selects image-search mode at the root.
    pub image_hash: Option<String>,
    /// `prev` page cursor.
    pub previous: Option<String>,
    /// `next` page cursor.
    pub next: Option<String>,
    /// `seek` date cursor.
    pub seek: Option<String>,
    /// `range` text parsed as a signed 32-bit integer, defaulting to zero.
    pub range: Option<String>,
    /// `f_cats` remote exclusion mask, inverted into the Android category mask.
    pub excluded_categories: Option<String>,
    /// `f_search` raw search expression.
    pub search: Option<String>,
    /// `advsearch`; only the exact value `1` enables advanced-field interpretation.
    pub advanced_search: Option<String>,
    /// `f_sh`; only the exact value `on` enables the name option.
    pub search_name: Option<String>,
    /// `f_sto`; only the exact value `on` enables the tags option.
    pub search_tags: Option<String>,
    /// `f_sfl`; only the exact value `on` enables the description option.
    pub search_description: Option<String>,
    /// `f_sfu`; only the exact value `on` enables the torrent-filename option.
    pub search_torrent_filenames: Option<String>,
    /// `f_sft`; only the exact value `on` enables the low-power-tags option.
    pub search_low_power_tags: Option<String>,
    /// `f_srdd` minimum-rating text.
    pub minimum_rating: Option<String>,
    /// `f_spf` minimum page-count text.
    pub page_from: Option<String>,
    /// `f_spt` maximum page-count text.
    pub page_to: Option<String>,
    /// `tl` toplist identifier.
    pub toplist: Option<String>,
    /// `p` toplist page cursor.
    pub toplist_page: Option<String>,
}

/// Sanitized output of Android's URL syntax and exact-host preflight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlSnapshot {
    /// At most the first two meaningful, once-decoded Ktor path segments.
    pub segments: Vec<String>,
    /// First decoded values for only the query fields relevant to Gallery List state.
    pub parameters: GalleryListUrlParameters,
}

/// Versioned request consumed by the sanitized Gallery List URL parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlParseRequest {
    /// Protocol version; callers must use [`API_VERSION`].
    pub version: u8,
    /// URL facts already restricted by Android's syntax and exact-host gate.
    pub snapshot: GalleryListUrlSnapshot,
}

/// Complete platform-neutral state of the existing Kotlin `ListUrlBuilder`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlState {
    /// Legacy integer mode discriminant.
    pub mode: i32,
    /// Previous-page cursor.
    pub previous: Option<String>,
    /// Next-page cursor.
    pub next: Option<String>,
    /// Date or toplist page cursor.
    pub seek: Option<String>,
    /// Parsed range, defaulting to zero.
    pub range: i32,
    /// Android category mask, or `-1` when absent/invalid.
    pub category: i32,
    /// Search, uploader, tag, or toplist value.
    pub keyword: Option<String>,
    /// Image-search hash when root `f_shash` is present.
    pub image_hash: Option<String>,
    /// Language index; URL parsing does not infer one and always returns `-1`.
    pub language: i32,
    /// Parsed advanced-search option mask, or `-1` when disabled.
    pub advanced_search: i32,
    /// Parsed minimum rating, or `-1` when disabled/invalid.
    pub minimum_rating: i32,
    /// Parsed minimum page count, or `-1` when disabled/invalid.
    pub page_from: i32,
    /// Parsed maximum page count, or `-1` when disabled/invalid.
    pub page_to: i32,
}

/// V1 parse payload; `state` is absent when the legacy parser would reject the URL shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlParsePlan {
    /// Parsed builder state, or `None` for an unsupported path/toplist/category.
    pub state: Option<GalleryListUrlState>,
}

/// Parses one sanitized snapshot into a versioned success/error envelope.
#[must_use]
pub fn parse_envelope(
    request: &GalleryListUrlParseRequest,
) -> ApiEnvelope<GalleryListUrlParsePlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error("unsupported gallery-list URL parser protocol version".to_owned()),
        );
    }
    match parse(&request.snapshot) {
        Ok(state) => ApiEnvelope::ok(API_VERSION, MODULE, GalleryListUrlParsePlan { state }),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Interprets sanitized Ktor URL components as legacy Gallery List builder state.
///
/// # Errors
///
/// Returns [`RustError::Error`] for a forged snapshot with more than two segments
/// or an invalid second-stage percent escape in an uploader/tag value. Error text
/// never includes the path or query value.
pub fn parse(snapshot: &GalleryListUrlSnapshot) -> Result<Option<GalleryListUrlState>, RustError> {
    if snapshot.segments.len() > 2 {
        return Err(RustError::Error(
            "gallery-list URL snapshot has too many path segments".to_owned(),
        ));
    }
    let Some(head) = snapshot.segments.first() else {
        let mut state = state_from_parameters(&snapshot.parameters);
        if let Some(image_hash) = snapshot.parameters.image_hash.clone() {
            state.mode = MODE_IMAGE_SEARCH;
            state.image_hash = Some(image_hash);
        }
        return Ok(Some(state));
    };

    match head.as_str() {
        "uploader" | "tag" => {
            let Some(value) = snapshot.segments.get(1) else {
                return Ok(None);
            };
            let mut state = state_from_parameters(&snapshot.parameters);
            state.mode = if head == "uploader" {
                MODE_UPLOADER
            } else {
                MODE_TAG
            };
            state.keyword = Some(decode_query_component(value)?);
            Ok(Some(state))
        }
        "toplist.php" => Ok(parse_toplist(&snapshot.parameters)),
        category_name => {
            let Some(category) = category(category_name) else {
                return Ok(None);
            };
            let mut state = state_from_parameters(&snapshot.parameters);
            state.category = category;
            Ok(Some(state))
        }
    }
}

fn state_from_parameters(parameters: &GalleryListUrlParameters) -> GalleryListUrlState {
    let mut state = GalleryListUrlState {
        mode: MODE_NORMAL,
        previous: parameters.previous.clone(),
        next: parameters.next.clone(),
        seek: parameters.seek.clone(),
        range: parse_i32(parameters.range.as_deref(), 0),
        category: parameters
            .excluded_categories
            .as_deref()
            .and_then(|value| value.parse::<i32>().ok())
            .map_or(-1, invert_category),
        keyword: parameters.search.clone(),
        image_hash: None,
        language: -1,
        advanced_search: -1,
        minimum_rating: -1,
        page_from: -1,
        page_to: -1,
    };
    if parameters.advanced_search.as_deref() == Some("1") {
        state.advanced_search = 0;
        for (value, mask) in [
            (parameters.search_name.as_deref(), ADVANCED_SEARCH_SH),
            (parameters.search_tags.as_deref(), ADVANCED_SEARCH_STO),
            (
                parameters.search_description.as_deref(),
                ADVANCED_SEARCH_SFL,
            ),
            (
                parameters.search_torrent_filenames.as_deref(),
                ADVANCED_SEARCH_SFU,
            ),
            (
                parameters.search_low_power_tags.as_deref(),
                ADVANCED_SEARCH_SFT,
            ),
        ] {
            if value == Some("on") {
                state.advanced_search |= mask;
            }
        }
        state.minimum_rating = parse_i32(parameters.minimum_rating.as_deref(), -1);
        state.page_from = parse_i32(parameters.page_from.as_deref(), -1);
        state.page_to = parse_i32(parameters.page_to.as_deref(), -1);
    }
    state
}

fn parse_toplist(parameters: &GalleryListUrlParameters) -> Option<GalleryListUrlState> {
    let toplist = parameters.toplist.as_deref()?;
    if !matches!(toplist, "11" | "12" | "13" | "15") {
        return None;
    }
    Some(GalleryListUrlState {
        mode: MODE_TOPLIST,
        previous: None,
        next: None,
        seek: parameters.toplist_page.clone(),
        range: 0,
        category: -1,
        keyword: Some(toplist.to_owned()),
        image_hash: None,
        language: -1,
        advanced_search: -1,
        minimum_rating: -1,
        page_from: -1,
        page_to: -1,
    })
}

fn category(value: &str) -> Option<i32> {
    let aliases: &[(&[&str], i32)] = &[
        (&["misc"], CATEGORY_MISC),
        (&["doujinshi"], CATEGORY_DOUJINSHI),
        (&["manga"], CATEGORY_MANGA),
        (
            &["artistcg", "Artist CG Sets", "Artist CG"],
            CATEGORY_ARTIST_CG,
        ),
        (&["gamecg", "Game CG Sets", "Game CG"], CATEGORY_GAME_CG),
        (&["imageset", "Image Sets", "Image Set"], CATEGORY_IMAGE_SET),
        (&["cosplay"], CATEGORY_COSPLAY),
        (&["asianporn", "Asian Porn"], CATEGORY_ASIAN_PORN),
        (&["non-h"], CATEGORY_NON_H),
        (&["western"], CATEGORY_WESTERN),
        (&["private"], CATEGORY_PRIVATE),
    ];
    aliases.iter().find_map(|(names, category)| {
        names
            .iter()
            .any(|name| value.eq_ignore_ascii_case(name))
            .then_some(*category)
    })
}

fn decode_query_component(value: &str) -> Result<String, RustError> {
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        let bytes = value.as_bytes();
        match bytes[index] {
            b'+' => {
                decoded.push(' ');
                index += 1;
            }
            b'%' => {
                let mut stripe = Vec::new();
                while index < value.len() && value.as_bytes()[index] == b'%' {
                    if index + 2 >= value.len() {
                        return invalid_path_value();
                    }
                    let high = hex(value.as_bytes()[index + 1]);
                    let low = hex(value.as_bytes()[index + 2]);
                    let (Some(high), Some(low)) = (high, low) else {
                        return invalid_path_value();
                    };
                    stripe.push((high << 4) | low);
                    index += 3;
                }
                decoded.push_str(&String::from_utf8_lossy(&stripe));
            }
            _ => {
                let Some(character) = value[index..].chars().next() else {
                    return invalid_path_value();
                };
                decoded.push(character);
                index += character.len_utf8();
            }
        }
    }
    Ok(decoded)
}

fn invalid_path_value<T>() -> Result<T, RustError> {
    Err(RustError::Error(
        "invalid encoded gallery-list path value".to_owned(),
    ))
}

const fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_i32(value: Option<&str>, default: i32) -> i32 {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

const fn invert_category(category: i32) -> i32 {
    !category & ALL_CATEGORIES
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::RustError;
    use serde_json::json;

    use super::{
        API_VERSION, GalleryListUrlParameters, GalleryListUrlParsePlan, GalleryListUrlParseRequest,
        GalleryListUrlSnapshot, GalleryListUrlState, MODULE, parse, parse_envelope,
    };

    #[test]
    fn parses_root_image_search_and_complete_advanced_parameters() {
        let mut snapshot = snapshot();
        snapshot.parameters = GalleryListUrlParameters {
            image_hash: Some("hash".to_owned()),
            previous: Some("10".to_owned()),
            next: Some("20".to_owned()),
            seek: Some("2026-08-03".to_owned()),
            range: Some("15".to_owned()),
            excluded_categories: Some("767".to_owned()),
            search: Some("cats".to_owned()),
            advanced_search: Some("1".to_owned()),
            search_name: Some("on".to_owned()),
            search_tags: Some("on".to_owned()),
            search_description: Some("on".to_owned()),
            search_torrent_filenames: Some("on".to_owned()),
            search_low_power_tags: Some("on".to_owned()),
            minimum_rating: Some("4".to_owned()),
            page_from: Some("10".to_owned()),
            page_to: Some("200".to_owned()),
            toplist: None,
            toplist_page: None,
        };
        assert_eq!(
            parse(&snapshot).expect("valid root snapshot should parse"),
            Some(GalleryListUrlState {
                mode: 4,
                previous: Some("10".to_owned()),
                next: Some("20".to_owned()),
                seek: Some("2026-08-03".to_owned()),
                range: 15,
                category: 256,
                keyword: Some("cats".to_owned()),
                image_hash: Some("hash".to_owned()),
                language: -1,
                advanced_search: 31,
                minimum_rating: 4,
                page_from: 10,
                page_to: 200,
            })
        );

        let mut empty_hash = GalleryListUrlSnapshot {
            segments: Vec::new(),
            parameters: parameters(),
        };
        empty_hash.parameters.image_hash = Some(String::new());
        let state = parse(&empty_hash)
            .expect("empty image hash presence should parse")
            .expect("root should be supported");
        assert_eq!(state.mode, 4);
        assert_eq!(state.image_hash.as_deref(), Some(""));
    }

    #[test]
    fn preserves_parameter_defaults_and_exact_advanced_markers() {
        let mut snapshot = snapshot();
        snapshot.parameters.range = Some("invalid".to_owned());
        snapshot.parameters.excluded_categories = Some("2147483648".to_owned());
        snapshot.parameters.advanced_search = Some("01".to_owned());
        snapshot.parameters.minimum_rating = Some("5".to_owned());
        let state = parse(&snapshot)
            .expect("invalid optional numbers should use defaults")
            .expect("root should parse");
        assert_eq!(state.range, 0);
        assert_eq!(state.category, -1);
        assert_eq!(state.advanced_search, -1);
        assert_eq!(state.minimum_rating, -1);

        snapshot.parameters.advanced_search = Some("1".to_owned());
        snapshot.parameters.search_name = Some("ON".to_owned());
        snapshot.parameters.search_tags = Some("on".to_owned());
        snapshot.parameters.minimum_rating = Some("+5".to_owned());
        snapshot.parameters.page_from = Some("-2".to_owned());
        snapshot.parameters.page_to = Some("bad".to_owned());
        let state = parse(&snapshot)
            .expect("enabled advanced fields should parse")
            .expect("root should parse");
        assert_eq!(state.advanced_search, 2);
        assert_eq!(state.minimum_rating, 5);
        assert_eq!(state.page_from, -2);
        assert_eq!(state.page_to, -1);

        for (excluded, expected) in [("0", 1023), ("1023", 0), ("-1", 0)] {
            snapshot.parameters.excluded_categories = Some(excluded.to_owned());
            let state = parse(&snapshot)
                .expect("signed category mask should parse")
                .expect("root should parse");
            assert_eq!(state.category, expected);
        }
    }

    #[test]
    fn parses_named_paths_with_the_legacy_second_decode() {
        let mut snapshot = snapshot();
        snapshot.segments = vec!["uploader".to_owned(), "alice+%2520%E6%97%A5".to_owned()];
        snapshot.parameters.search = Some("ignored replacement".to_owned());
        let state = parse(&snapshot)
            .expect("encoded uploader should parse")
            .expect("uploader should be supported");
        assert_eq!(state.mode, 1);
        assert_eq!(state.keyword.as_deref(), Some("alice %20日"));

        snapshot.segments = vec!["tag".to_owned(), String::new()];
        let state = parse(&snapshot)
            .expect("empty interior tag segment should parse")
            .expect("tag should be supported");
        assert_eq!(state.mode, 2);
        assert_eq!(state.keyword.as_deref(), Some(""));

        snapshot.segments = vec!["tag".to_owned(), "%FF".to_owned()];
        let state = parse(&snapshot)
            .expect("invalid UTF-8 bytes use Ktor replacement semantics")
            .expect("tag should be supported");
        assert_eq!(state.keyword.as_deref(), Some("�"));

        snapshot.segments = vec!["tag".to_owned()];
        assert_eq!(
            parse(&snapshot).expect("missing tag value is unsupported"),
            None
        );
    }

    #[test]
    fn accepts_only_legacy_toplists_and_category_aliases() {
        for toplist in ["11", "12", "13", "15"] {
            let mut snapshot = snapshot();
            snapshot.segments = vec!["toplist.php".to_owned()];
            snapshot.parameters.toplist = Some(toplist.to_owned());
            snapshot.parameters.toplist_page = Some("2".to_owned());
            snapshot.parameters.previous = Some("ignored".to_owned());
            snapshot.parameters.search = Some("ignored".to_owned());
            snapshot.parameters.advanced_search = Some("1".to_owned());
            snapshot.parameters.search_name = Some("on".to_owned());
            let state = parse(&snapshot)
                .expect("toplist should parse")
                .expect("whitelisted toplist should be supported");
            assert_eq!(state.mode, 6);
            assert_eq!(state.keyword.as_deref(), Some(toplist));
            assert_eq!(state.seek.as_deref(), Some("2"));
            assert_eq!(state.previous, None);
            assert_eq!(state.advanced_search, -1);
        }
        for toplist in [None, Some("10"), Some("14"), Some("")] {
            let mut snapshot = snapshot();
            snapshot.segments = vec!["toplist.php".to_owned()];
            snapshot.parameters.toplist = toplist.map(str::to_owned);
            assert_eq!(parse(&snapshot).expect("toplist shape should parse"), None);
        }

        let categories = [
            ("misc", 1),
            ("DOUJINSHI", 2),
            ("manga", 4),
            ("Artist CG Sets", 8),
            ("game cg", 16),
            ("Image Set", 32),
            ("cosplay", 64),
            ("ASIAN PORN", 128),
            ("non-h", 256),
            ("western", 512),
            ("private", 1024),
        ];
        for (name, expected) in categories {
            let mut snapshot = snapshot();
            snapshot.segments = vec![name.to_owned()];
            let state = parse(&snapshot)
                .expect("category should parse")
                .expect("category alias should be supported");
            assert_eq!(state.category, expected);
        }
        for unsupported in ["unknown", "watched", "popular", ""] {
            let mut snapshot = snapshot();
            snapshot.segments = vec![unsupported.to_owned()];
            assert_eq!(
                parse(&snapshot).expect("unsupported path should parse"),
                None
            );
        }
    }

    #[test]
    fn rejects_forged_segments_and_invalid_second_stage_escapes() {
        let mut snapshot = snapshot();
        snapshot.segments = vec![
            "manga".to_owned(),
            "ignored".to_owned(),
            "secret".to_owned(),
        ];
        assert_eq!(
            parse(&snapshot),
            Err(RustError::Error(
                "gallery-list URL snapshot has too many path segments".to_owned()
            ))
        );

        for invalid in ["%", "%1", "%GG", "%E6%ZZ"] {
            snapshot.segments = vec!["tag".to_owned(), invalid.to_owned()];
            assert_eq!(
                parse(&snapshot),
                Err(RustError::Error(
                    "invalid encoded gallery-list path value".to_owned()
                ))
            );
        }
    }

    #[test]
    fn request_and_response_have_stable_json_and_cbor_contracts() {
        let request = GalleryListUrlParseRequest {
            version: API_VERSION,
            snapshot: GalleryListUrlSnapshot {
                segments: vec!["tag".to_owned(), "artist%3Aalice".to_owned()],
                parameters: parameters(),
            },
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request JSON should encode"),
            json!({
                "version": 1,
                "snapshot": {
                    "segments": ["tag", "artist%3Aalice"],
                    "parameters": {
                        "imageHash": null,
                        "previous": null,
                        "next": null,
                        "seek": null,
                        "range": null,
                        "excludedCategories": null,
                        "search": null,
                        "advancedSearch": null,
                        "searchName": null,
                        "searchTags": null,
                        "searchDescription": null,
                        "searchTorrentFilenames": null,
                        "searchLowPowerTags": null,
                        "minimumRating": null,
                        "pageFrom": null,
                        "pageTo": null,
                        "toplist": null,
                        "toplistPage": null,
                    }
                }
            })
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&request).expect("V1 request should encode")),
            "a26776657273696f6e0168736e617073686f74a2687365676d656e747382637461676e617274697374253341616c6963656a706172616d6574657273b269696d61676548617368f66870726576696f7573f6646e657874f6647365656bf66572616e6765f6726578636c7564656443617465676f72696573f666736561726368f66e616476616e636564536561726368f66a7365617263684e616d65f66a73656172636854616773f6717365617263684465736372697074696f6ef676736561726368546f7272656e7446696c656e616d6573f6727365617263684c6f77506f77657254616773f66d6d696e696d756d526174696e67f6687061676546726f6df66670616765546ff667746f706c697374f66b746f706c69737450616765f6"
        );

        let envelope = parse_envelope(&request);
        assert_eq!(
            envelope.payload,
            Some(GalleryListUrlParsePlan {
                state: Some(GalleryListUrlState {
                    mode: 2,
                    previous: None,
                    next: None,
                    seek: None,
                    range: 0,
                    category: -1,
                    keyword: Some("artist:alice".to_owned()),
                    image_hash: None,
                    language: -1,
                    advanced_search: -1,
                    minimum_rating: -1,
                    page_from: -1,
                    page_to: -1,
                })
            })
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            "a46776657273696f6e01666d6f64756c657467616c6c6572794c69737455726c506172736572677061796c6f6164a1657374617465ad646d6f6465026870726576696f7573f6646e657874f6647365656bf66572616e6765006863617465676f727920676b6579776f72646c6172746973743a616c69636569696d61676548617368f6686c616e6775616765206e616476616e636564536561726368206d6d696e696d756d526174696e6720687061676546726f6d206670616765546f20656572726f72f6"
        );
    }

    #[test]
    fn envelope_contains_version_and_sanitized_errors() {
        let mut request = GalleryListUrlParseRequest {
            version: 2,
            snapshot: snapshot(),
        };
        let envelope = parse_envelope(&request);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(
                "unsupported gallery-list URL parser protocol version".to_owned()
            ))
        );

        request.version = API_VERSION;
        request.snapshot.segments = vec!["tag".to_owned(), "%GG".to_owned()];
        assert_eq!(
            parse_envelope(&request).error,
            Some(RustError::Error(
                "invalid encoded gallery-list path value".to_owned()
            ))
        );
    }

    fn snapshot() -> GalleryListUrlSnapshot {
        GalleryListUrlSnapshot {
            segments: Vec::new(),
            parameters: parameters(),
        }
    }

    fn parameters() -> GalleryListUrlParameters {
        GalleryListUrlParameters {
            image_hash: None,
            previous: None,
            next: None,
            seek: None,
            range: None,
            excluded_categories: None,
            search: None,
            advanced_search: None,
            search_name: None,
            search_tags: None,
            search_description: None,
            search_torrent_filenames: None,
            search_low_power_tags: None,
            minimum_rating: None,
            page_from: None,
            page_to: None,
            toplist: None,
            toplist_page: None,
        }
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        hex
    }
}
