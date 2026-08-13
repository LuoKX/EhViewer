//! Canonical gallery category and similar-title compatibility rules.
//!
//! Android retains localized presentation, category colors, settings, and
//! navigation. This module owns the portable category taxonomy used by parsers
//! and URL planners plus the historical title cleanup used by similar search.

use std::sync::LazyLock;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// Protocol version of the similar-gallery title request and response.
pub const GALLERY_TITLE_API_VERSION: u8 = 1;

/// Stable module identifier used by the similar-gallery title envelope.
pub const GALLERY_TITLE_MODULE: &str = "galleryTitle";

/// Homepage/no-category sentinel retained by Android navigation state.
pub const CATEGORY_NONE: i32 = -1;
/// Miscellaneous category bit.
pub const CATEGORY_MISC: i32 = 0x1;
/// Doujinshi category bit.
pub const CATEGORY_DOUJINSHI: i32 = 0x2;
/// Manga category bit.
pub const CATEGORY_MANGA: i32 = 0x4;
/// Artist CG category bit.
pub const CATEGORY_ARTIST_CG: i32 = 0x8;
/// Game CG category bit.
pub const CATEGORY_GAME_CG: i32 = 0x10;
/// Image set category bit.
pub const CATEGORY_IMAGE_SET: i32 = 0x20;
/// Cosplay category bit.
pub const CATEGORY_COSPLAY: i32 = 0x40;
/// Asian pornography category bit.
pub const CATEGORY_ASIAN_PORN: i32 = 0x80;
/// Non-H category bit.
pub const CATEGORY_NON_H: i32 = 0x100;
/// Western category bit.
pub const CATEGORY_WESTERN: i32 = 0x200;
/// Mask of the ten selectable Gallery List categories.
pub const ALL_CATEGORIES: i32 = 0x3ff;
/// Private-gallery category bit.
pub const CATEGORY_PRIVATE: i32 = 0x400;
/// Unknown-category sentinel bit.
pub const CATEGORY_UNKNOWN: i32 = 0x800;

struct CategoryDefinition {
    value: i32,
    names: &'static [&'static str],
}

const CATEGORY_DEFINITIONS: &[CategoryDefinition] = &[
    CategoryDefinition {
        value: CATEGORY_MISC,
        names: &["misc"],
    },
    CategoryDefinition {
        value: CATEGORY_DOUJINSHI,
        names: &["doujinshi"],
    },
    CategoryDefinition {
        value: CATEGORY_MANGA,
        names: &["manga"],
    },
    CategoryDefinition {
        value: CATEGORY_ARTIST_CG,
        names: &["artistcg", "Artist CG Sets", "Artist CG"],
    },
    CategoryDefinition {
        value: CATEGORY_GAME_CG,
        names: &["gamecg", "Game CG Sets", "Game CG"],
    },
    CategoryDefinition {
        value: CATEGORY_IMAGE_SET,
        names: &["imageset", "Image Sets", "Image Set"],
    },
    CategoryDefinition {
        value: CATEGORY_COSPLAY,
        names: &["cosplay"],
    },
    CategoryDefinition {
        value: CATEGORY_ASIAN_PORN,
        names: &["asianporn", "Asian Porn"],
    },
    CategoryDefinition {
        value: CATEGORY_NON_H,
        names: &["non-h"],
    },
    CategoryDefinition {
        value: CATEGORY_WESTERN,
        names: &["western"],
    },
    CategoryDefinition {
        value: CATEGORY_PRIVATE,
        names: &["private"],
    },
    CategoryDefinition {
        value: CATEGORY_UNKNOWN,
        names: &["unknown"],
    },
];

static TITLE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\([^)]*\)|\[[^]]*]|\{[^}]*\}|~[^~]*~|\s+)*")
        .expect("source-controlled Gallery title prefix must compile")
});

static TITLE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i:(?:\s+ch[^\n\r\u{0085}\u{2028}\u{2029}][\s\d-]+)?(?:\([^)]*\)|\[[^]]*]|\{[^}]*\}|~[^~]*~|\s+)*)(?:\r\n|[\n\r\u{0085}\u{2028}\u{2029}])?\z",
    )
        .expect("source-controlled Gallery title suffix must compile")
});

/// Versioned sanitized input for one similar-gallery title cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTitleRequest {
    /// Protocol version; callers must use [`GALLERY_TITLE_API_VERSION`].
    pub version: u8,
    /// Complete title, or `None` when Android has no title.
    pub input: Option<String>,
}

/// Complete pure result of one similar-gallery title cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryTitlePlan {
    /// Search title after compatibility cleanup, or `None` when nothing remains.
    pub title: Option<String>,
}

/// Resolves any canonical or historical category alias without trimming.
///
/// Matching is ASCII case-insensitive because every defined alias is ASCII.
#[must_use]
pub fn category_from_alias(category: &str) -> Option<i32> {
    CATEGORY_DEFINITIONS.iter().find_map(|definition| {
        definition
            .names
            .iter()
            .any(|name| category.eq_ignore_ascii_case(name))
            .then_some(definition.value)
    })
}

/// Converts a category label to its Android bit, defaulting to unknown.
///
/// The input is not trimmed. Existing HTML/JSON callers retain their own exact
/// preprocessing before entering this shared rule.
#[must_use]
pub fn category_value(category: &str) -> i32 {
    category_from_alias(category).unwrap_or(CATEGORY_UNKNOWN)
}

/// Returns the canonical display label for a category bit.
///
/// Unknown masks and sentinels use the historical `unknown` label.
#[must_use]
pub fn category_name(category: i32) -> &'static str {
    CATEGORY_DEFINITIONS
        .iter()
        .find(|definition| definition.value == category)
        .map_or("unknown", |definition| definition.names[0])
}

/// Converts a remote exclusion mask to the selectable Android category mask,
/// or performs the inverse conversion.
#[must_use]
pub const fn invert_category(category: i32) -> i32 {
    !category & ALL_CATEGORIES
}

/// Applies the historical similar-gallery title cleanup.
///
/// Prefix and suffix delimiter groups are removed first. The suffix preserves
/// the case-insensitive `ch.` wildcard grammar, then the substring before the
/// final pipe is trimmed. Empty output becomes `None`.
#[must_use]
pub fn extract_gallery_title(full_title: Option<&str>) -> Option<String> {
    let title = full_title?;
    let title = TITLE_PREFIX.replace(title, "");
    let title = TITLE_SUFFIX.replace(&title, "");
    let title = title
        .rsplit_once('|')
        .map_or(title.as_ref(), |(before, _)| before)
        .trim();
    (!title.is_empty()).then(|| title.to_owned())
}

/// Plans one versioned similar-gallery title cleanup.
#[must_use]
pub fn plan_gallery_title(request: &GalleryTitleRequest) -> GalleryTitlePlan {
    GalleryTitlePlan {
        title: extract_gallery_title(request.input.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use crate::contracts::ApiEnvelope;

    use super::*;

    #[test]
    fn category_bits_labels_and_aliases_match_the_android_golden() {
        let golden = [
            (CATEGORY_MISC, "misc", &["misc"][..]),
            (CATEGORY_DOUJINSHI, "doujinshi", &["doujinshi"][..]),
            (CATEGORY_MANGA, "manga", &["manga"][..]),
            (
                CATEGORY_ARTIST_CG,
                "artistcg",
                &["artistcg", "Artist CG Sets", "Artist CG"][..],
            ),
            (
                CATEGORY_GAME_CG,
                "gamecg",
                &["gamecg", "Game CG Sets", "Game CG"][..],
            ),
            (
                CATEGORY_IMAGE_SET,
                "imageset",
                &["imageset", "Image Sets", "Image Set"][..],
            ),
            (CATEGORY_COSPLAY, "cosplay", &["cosplay"][..]),
            (
                CATEGORY_ASIAN_PORN,
                "asianporn",
                &["asianporn", "Asian Porn"][..],
            ),
            (CATEGORY_NON_H, "non-h", &["non-h"][..]),
            (CATEGORY_WESTERN, "western", &["western"][..]),
            (CATEGORY_PRIVATE, "private", &["private"][..]),
            (CATEGORY_UNKNOWN, "unknown", &["unknown"][..]),
        ];

        assert_eq!(CATEGORY_NONE, -1);
        assert_eq!(ALL_CATEGORIES, 0x3ff);
        for (value, canonical, aliases) in golden {
            assert_eq!(category_name(value), canonical);
            for alias in aliases {
                assert_eq!(category_from_alias(alias), Some(value));
                assert_eq!(
                    category_from_alias(&alias.to_ascii_uppercase()),
                    Some(value)
                );
                assert_eq!(category_value(alias), value);
            }
        }
        assert_eq!(category_from_alias(" misc "), None);
        assert_eq!(category_value(" misc "), CATEGORY_UNKNOWN);
        assert_eq!(category_from_alias("missing"), None);
        assert_eq!(category_name(CATEGORY_NONE), "unknown");
    }

    #[test]
    fn category_inversion_masks_every_non_selectable_bit() {
        assert_eq!(invert_category(0), ALL_CATEGORIES);
        assert_eq!(invert_category(ALL_CATEGORIES), 0);
        assert_eq!(invert_category(CATEGORY_MISC), 0x3fe);
        assert_eq!(invert_category(-1), 0);
        assert_eq!(invert_category(CATEGORY_PRIVATE), ALL_CATEGORIES);
    }

    #[test]
    fn title_cleanup_preserves_delimiter_chapter_pipe_and_empty_rules() {
        let cases = [
            (None, None),
            (Some(""), None),
            (Some(" \t\n "), None),
            (Some("Plain title"), Some("Plain title")),
            (Some("[Group] (Event) {Edition} ~Raw~ Title"), Some("Title")),
            (Some("Title CHx 1-23 [Digital] (Complete)"), Some("Title")),
            (Some("Title ch😀 1-2"), Some("Title")),
            (Some("Title ch\r 1-2"), Some("Title ch\r 1-2")),
            (Some("Romaji | English | Other"), Some("Romaji | English")),
            (Some("[Only] {metadata} ~here~"), None),
            (Some("Title [Digital]\u{2028}"), Some("Title")),
            (
                Some("Title [Digital]\u{2028}\u{2028}"),
                Some("Title [Digital]"),
            ),
            (
                Some("\u{2003} Unicode title \u{2003}"),
                Some("Unicode title"),
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(extract_gallery_title(input), expected.map(str::to_owned));
        }
    }

    #[test]
    fn request_plan_and_envelope_have_stable_wire_shapes() {
        let request = GalleryTitleRequest {
            version: GALLERY_TITLE_API_VERSION,
            input: Some("[Group] Title | Translation CHx 1-2".to_owned()),
        };
        let plan = plan_gallery_title(&request);
        assert_eq!(plan.title.as_deref(), Some("Title"));

        let envelope = ApiEnvelope::ok(
            GALLERY_TITLE_API_VERSION,
            GALLERY_TITLE_MODULE,
            plan.clone(),
        );
        assert_eq!(
            serde_json::to_value(&request).expect("request JSON should encode"),
            serde_json::json!({
                "version": 1,
                "input": "[Group] Title | Translation CHx 1-2",
            })
        );
        assert_eq!(
            serde_json::to_value(&plan).expect("plan JSON should encode"),
            serde_json::json!({ "title": "Title" })
        );

        let request_hex =
            to_hex(&serde_cbor::to_vec(&request).expect("request CBOR should encode"));
        let envelope_hex =
            to_hex(&serde_cbor::to_vec(&envelope).expect("envelope CBOR should encode"));
        assert_eq!(
            request_hex,
            "a26776657273696f6e0165696e70757478235b47726f75705d205469746c65207c205472616e736c6174696f6e2043487820312d32"
        );
        assert_eq!(
            envelope_hex,
            "a46776657273696f6e01666d6f64756c656c67616c6c6572795469746c65677061796c6f6164a1657469746c65655469746c65656572726f72f6"
        );
    }

    fn to_hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;

        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        hex
    }
}
