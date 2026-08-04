//! Legacy JNI compatibility adapter for the migrated gallery-list parser.

use anyhow::Result;
use eh_domain::contracts::GalleryItemSummary;
use serde::Serialize;
use tl::{Parser, VDom};

use crate::regex;

mod legacy;

const EHGT_PREFIX: &str = "https://ehgt.org/";
const EX_PREFIX: &str = "https://s.exhentai.org/";

/// Converts a legacy category label to the Android category bitmask value.
///
/// This copy is intentionally independent from `eh_net`: both the Gallery List
/// rollback parser and the still-legacy Gallery Detail parser depend on its
/// historical behavior.
pub(crate) fn to_category_i32(category: &str) -> i32 {
    match category {
        "misc" => 0x1,
        "doujinshi" => 0x2,
        "manga" => 0x4,
        "artistcg" | "artist cg sets" | "artist cg" => 0x8,
        "gamecg" | "game cg sets" | "game cg" => 0x10,
        "imageset" | "image sets" | "image set" => 0x20,
        "cosplay" => 0x40,
        "asianporn" | "asian porn" => 0x80,
        "non-h" => 0x100,
        "western" => 0x200,
        "private" => 0x400,
        _ => 0x800,
    }
}

/// Removes the historical thumbnail host and `t/` prefix from a URL.
pub(crate) fn get_thumb_key(url: &str) -> String {
    url.trim_start_matches(EHGT_PREFIX)
        .trim_start_matches(EX_PREFIX)
        .trim_start_matches("t/")
        .to_owned()
}

/// Extracts the legacy gallery ID and access token from a supported URL.
pub(crate) fn parse_token_and_gid(url: &str) -> Option<(i64, String)> {
    let captures =
        regex!("https?://(?:exhentai.org|e-hentai.org(?:/lofi)?)/(?:g|mpv)/(\\d+)/([0-9a-f]{10})")
            .captures(url)?;
    Some((captures[1].parse().ok()?, captures[2].to_owned()))
}

/// Existing Kotlin `BaseGalleryInfo` CBOR shape.
///
/// New Rust code uses [`GalleryItemSummary`]. This compatibility DTO remains
/// until the Android gallery-list adapter consumes the versioned domain type.
#[derive(Serialize, Debug, Default)]
#[allow(non_snake_case)]
pub struct BaseGalleryInfo {
    pub gid: i64,
    pub token: String,
    pub title: String,
    pub titleJpn: Option<String>,
    pub thumbKey: String,
    pub category: i32,
    pub posted: String,
    pub uploader: Option<String>,
    pub disowned: bool,
    pub rating: f32,
    pub rated: bool,
    pub simpleTags: Vec<String>,
    pub pages: i32,
    pub thumbWidth: i32,
    pub thumbHeight: i32,
    pub simpleLanguage: Option<String>,
    pub favoriteSlot: i32,
    pub favoriteName: Option<String>,
    pub favoriteNote: Option<String>,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct GalleryListResult {
    prev: Option<String>,
    next: Option<String>,
    galleryInfoList: Vec<BaseGalleryInfo>,
}

/// Runs the frozen legacy gallery-list parser and returns its existing Kotlin
/// CBOR contract.
///
/// # Errors
///
/// Errors retain the legacy `EhError` tuple serialization. New code must use the
/// versioned `parseGalleryInfoListV1` JNI method instead.
pub fn parse_info_list(dom: &VDom, parser: &Parser) -> Result<GalleryListResult> {
    legacy::parse_info_list(dom, parser)
}

impl From<GalleryItemSummary> for BaseGalleryInfo {
    fn from(item: GalleryItemSummary) -> Self {
        Self {
            gid: item.gallery_id,
            token: item.token,
            title: item.title,
            titleJpn: item.title_japanese,
            thumbKey: item.thumbnail_key,
            category: item.category,
            posted: item.posted,
            uploader: item.uploader,
            disowned: item.disowned,
            rating: item.rating,
            rated: item.rated,
            simpleTags: item.tags,
            pages: item.page_count,
            thumbWidth: item.thumbnail_width,
            thumbHeight: item.thumbnail_height,
            simpleLanguage: item.language,
            favoriteSlot: item.favorite_slot,
            favoriteName: item.favorite_name,
            favoriteNote: item.favorite_note,
        }
    }
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::GalleryItemSummary;
    use tl::ParserOptions;

    use super::{BaseGalleryInfo, GalleryListResult, parse_info_list};
    use crate::EhError;

    #[test]
    fn compatibility_item_keeps_the_existing_cbor_field_shape() {
        let domain = GalleryItemSummary {
            gallery_id: 42,
            title: "Title".to_owned(),
            token: "0123456789".to_owned(),
            title_japanese: None,
            thumbnail_key: "thumb.jpg".to_owned(),
            category: 4,
            posted: "2026-08-02".to_owned(),
            uploader: None,
            disowned: false,
            rating: 4.5,
            rated: true,
            tags: vec!["tag".to_owned()],
            page_count: 12,
            thumbnail_width: 150,
            thumbnail_height: 200,
            language: None,
            favorite_slot: -2,
            favorite_name: None,
            favorite_note: None,
        };
        let legacy = BaseGalleryInfo::from(domain.clone());

        assert_eq!(
            serde_cbor::value::to_value(domain).expect("domain DTO should serialize"),
            serde_cbor::value::to_value(legacy).expect("legacy DTO should serialize")
        );
    }

    #[test]
    fn legacy_errors_remain_downcastable_for_jni_serialization() {
        let dom = tl::parse(
            "<!doctype html><html><body></body></html>",
            ParserOptions::default(),
        )
        .expect("test HTML should parse");
        let error = parse_info_list(&dom, dom.parser()).expect_err("empty list should fail");
        assert!(matches!(
            error.downcast_ref::<EhError>(),
            Some(EhError::NoHits)
        ));
    }

    #[test]
    fn legacy_and_core_parsers_keep_the_same_success_cbor_shape() {
        let html = include_str!("../../crates/net/test_data/gallery_list.html");
        let dom = tl::parse(html, ParserOptions::default()).expect("fixture HTML should parse");
        let legacy =
            parse_info_list(&dom, dom.parser()).expect("legacy parser should parse fixture");
        let core = eh_net::gallery::list::parse(html).expect("core parser should parse fixture");
        let adapted = GalleryListResult {
            prev: core.previous,
            next: core.next,
            galleryInfoList: core.items.into_iter().map(Into::into).collect(),
        };

        assert_eq!(
            serde_cbor::value::to_value(legacy).expect("legacy result should serialize"),
            serde_cbor::value::to_value(adapted).expect("core result should serialize")
        );
    }
}
