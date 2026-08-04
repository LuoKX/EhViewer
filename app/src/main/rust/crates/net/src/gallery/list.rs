//! Gallery-list response parsing.

use std::borrow::Cow;
use std::ops::Index;

use eh_domain::contracts::{ApiEnvelope, GalleryItemSummary, GalleryPage, RustError};
use tl::{Node, Parser, ParserOptions, VDom};

use crate::html::{
    child_by_id, first_by_class, first_child_by_class, first_descendant_attr, handle_attr,
    node_attr,
};
use crate::text::unescape;

const EHGT_PREFIX: &str = "https://ehgt.org/";
const EX_PREFIX: &str = "https://s.exhentai.org/";

/// Current gallery-list bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryList";

/// Parses gallery-list HTML and always returns a versioned success/error
/// envelope suitable for the V1 JNI boundary.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<GalleryPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses one gallery-list HTML response into the stable domain contract.
///
/// # Errors
///
/// Returns [`RustError::NoWatched`] for the watched-tags empty state,
/// [`RustError::NoHits`] when no complete gallery entries are present, and
/// [`RustError::IpBanned`] when the response is not an HTML document.
/// [`RustError::Error`] represents a malformed document or remote warning.
pub fn parse(html: &str) -> Result<GalleryPage, RustError> {
    let dom = tl::parse(html, ParserOptions::default())
        .map_err(|error| RustError::Error(format!("invalid gallery-list HTML: {error}")))?;
    if dom.version().is_none() {
        return Err(RustError::IpBanned(html.to_owned()));
    }
    parse_dom(&dom, dom.parser())
}

/// Parses a pre-built HTML DOM while preserving the legacy JNI parser's single
/// parse pass.
///
/// # Errors
///
/// Returns the same typed domain failures as [`parse`]. Android/JNI adapters
/// must map those failures to their versioned boundary representation.
pub fn parse_dom(dom: &VDom<'_>, parser: &Parser<'_>) -> Result<GalleryPage, RustError> {
    if let Some(warning) = first_by_class(dom, "searchwarn") {
        let text = warning.inner_text(parser);
        return if text.contains("You do not have any watched tags") {
            Err(RustError::NoWatched)
        } else {
            Err(RustError::Error(text.into_owned()))
        };
    }

    let items = first_by_class(dom, "itg")
        .and_then(Node::children)
        .map(|children| {
            children
                .top()
                .iter()
                .filter_map(|handle| parse_item(handle.get(parser)?, parser))
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .ok_or(RustError::NoHits)?;

    Ok(GalleryPage {
        previous: page_cursor(dom, parser, "uprev", "prev"),
        next: page_cursor(dom, parser, "unext", "next"),
        items,
    })
}

/// Converts the remote category label to the existing Android category bit.
#[must_use]
pub fn category_value(category: &str) -> i32 {
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

/// Removes the known gallery-thumbnail host and `t/` prefix from a URL.
#[must_use]
pub fn thumbnail_key(url: &str) -> String {
    url.trim_start_matches(EHGT_PREFIX)
        .trim_start_matches(EX_PREFIX)
        .trim_start_matches("t/")
        .to_owned()
}

/// Extracts the numeric gallery ID and ten-character access token from a
/// supported gallery or multi-page-viewer URL.
#[must_use]
pub fn gallery_key(url: &str) -> Option<(i64, String)> {
    let captures =
        regex!("https?://(?:exhentai.org|e-hentai.org(?:/lofi)?)/(?:g|mpv)/(\\d+)/([0-9a-f]{10})")
            .captures(url)?;
    Some((captures[1].parse().ok()?, captures[2].to_owned()))
}

fn parse_item(node: &Node<'_>, parser: &Parser<'_>) -> Option<GalleryItemSummary> {
    let tag = node.as_tag()?;
    let title = first_child_by_class(node, parser, "glink")?.inner_text(parser);
    let name = first_child_by_class(node, parser, "glname")?;
    let detail_url = first_descendant_attr(name, parser, "href")
        .or_else(|| first_descendant_attr(node, parser, "href"))?;
    let (gallery_id, token) = gallery_key(detail_url)?;

    let tags = tag
        .query_selector(parser, ".gt, .gtl")?
        .filter_map(|handle| handle_attr(handle, parser, "title").map(str::to_owned))
        .collect();
    let (thumbnail, (thumbnail_height, thumbnail_width)) = parse_thumbnail(tag, parser)?;

    let category = first_child_by_class(node, parser, "cn")
        .or_else(|| first_child_by_class(node, parser, "cs"))
        .map_or_else(
            || Cow::Borrowed("unknown"),
            |value| value.inner_text(parser),
        );
    let posted_id = format!("posted_{gallery_id}");
    let (posted, favorite_name) = child_by_id(node, parser, &posted_id).map_or_else(
        || (String::new(), None),
        |posted_node| {
            (
                posted_node.inner_text(parser).trim().to_owned(),
                node_attr(posted_node, "title")
                    .map(|value| unescape(value).as_deref().unwrap_or(value).to_owned()),
            )
        },
    );

    let rating_node = first_child_by_class(node, parser, "ir")?
        .as_tag()?
        .attributes();
    let rating_class = rating_node.class()?.try_as_utf8_str()?;
    let rating_style = rating_node.get("style")??.try_as_utf8_str()?;
    let (uploader, disowned, page_count) = tag
        .query_selector(parser, ".glhide, .gl3e, .gl5t")?
        .next()
        .and_then(|handle| handle.get(parser))
        .map_or((None, false, 0), |details| {
            parse_uploader_and_pages(&details.inner_html(parser))
        });
    let favorite_note_id = format!("favnote_{gallery_id}");
    let favorite_note = child_by_id(node, parser, &favorite_note_id).map(|value| {
        let text = value.inner_text(parser);
        unescape(&text).as_deref().unwrap_or(&text).to_owned()
    });

    Some(GalleryItemSummary {
        gallery_id,
        token,
        title: unescape(title.trim()).ok()?.into_owned(),
        title_japanese: None,
        thumbnail_key: thumbnail_key(thumbnail),
        category: category_value(&category.trim().to_lowercase()),
        posted,
        uploader,
        disowned,
        rating: parse_rating(rating_style),
        rated: rating_class.contains("irr")
            || rating_class.contains("irg")
            || rating_class.contains("irb"),
        tags,
        page_count,
        thumbnail_width,
        thumbnail_height,
        language: None,
        favorite_slot: if favorite_name.is_some() { 0 } else { -2 },
        favorite_name,
        favorite_note,
    })
}

fn parse_thumbnail<'a>(
    tag: &'a tl::HTMLTag<'_>,
    parser: &'a Parser<'_>,
) -> Option<(&'a str, (i32, i32))> {
    let handle = tag
        .query_selector(parser, "[data-src]")?
        .next()
        .map(|handle| (handle, "data-src"))
        .or_else(|| {
            tag.query_selector(parser, "[src]")?
                .next()
                .map(|handle| (handle, "src"))
        })?;
    let url = handle_attr(handle.0, parser, handle.1)?;
    let resolution = parse_thumbnail_resolution(handle_attr(handle.0, parser, "style")?);
    Some((url, resolution))
}

fn parse_rating(style: &str) -> f32 {
    let mut values = regex!("\\d+px")
        .find_iter(style)
        .filter_map(|value| value.as_str().trim_end_matches("px").parse::<i16>().ok());
    match (values.next(), values.next()) {
        (Some(horizontal), Some(vertical)) => {
            let whole = 5 - horizontal / 16;
            if vertical == 21 {
                f32::from(whole - 1) + 0.5
            } else {
                f32::from(whole)
            }
        }
        _ => -1.0,
    }
}

fn parse_uploader_and_pages(html: &str) -> (Option<String>, bool, i32) {
    let uploader =
        regex!(r#"<a href="https://e[x-]hentai.org/uploader/.*?">(.*?)</a>|(\(Disowned\))"#)
            .captures(html)
            .and_then(|captures| captures.get(1).or_else(|| captures.get(2)))
            .map(|value| {
                let value = value.as_str();
                unescape(value).as_deref().unwrap_or(value).to_owned()
            });
    let page_count = regex!(r"<div>(\d+) pages</div>")
        .captures(html)
        .and_then(|captures| captures[1].parse().ok())
        .unwrap_or(0);
    (uploader, html.contains("style=\"opacity:0.5\""), page_count)
}

fn parse_thumbnail_resolution(style: &str) -> (i32, i32) {
    regex!(r"height:(\d+)px;width:(\d+)px")
        .captures(style)
        .map_or((0, 0), |captures| {
            (
                captures[1].parse().unwrap_or(0),
                captures[2].parse().unwrap_or(0),
            )
        })
}

fn page_cursor(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
    element_id: &str,
    parameter: &str,
) -> Option<String> {
    let href = handle_attr(dom.get_element_by_id(element_id)?, parser, "href")?;
    regex!(r"(?:prev|next)=(\d+(?:-\d+)?)")
        .captures(href)
        .filter(|_| href.contains(parameter))
        .map(|captures| captures.index(1).to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{ApiEnvelope, GalleryItemSummary, GalleryPage, RustError};

    use super::{
        API_VERSION, MODULE, category_value, gallery_key, parse, parse_envelope, parse_rating,
        parse_thumbnail_resolution, thumbnail_key,
    };
    use crate::test_support::to_hex;

    const FULL_PAGE: &str = include_str!("../../test_data/gallery_list.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656b67616c6c6572794c697374677061796c6f6164a364",
        "707265766431302d32646e6578746232306f67616c6c657279496e666f4c69737481b36367696418",
        "2a657469746c65655469746c6565746f6b656e6a30313233343536373839687469746c654a706e69",
        "e697a5e69cace8aa9e687468756d624b6579697468756d622e6a70676863617465676f7279046670",
        "6f737465646a323032362d30382d30326875706c6f616465726875706c6f61646572686469736f77",
        "6e6564f566726174696e67f94480657261746564f56a73696d706c65546167738163746167657061",
        "6765730c6a7468756d62576964746818966b7468756d6248656967687418c86e73696d706c654c61",
        "6e6775616765684a6170616e6573656c6661766f72697465536c6f74006c6661766f726974654e61",
        "6d65694661766f72697465736c6661766f726974654e6f7465646e6f7465656572726f72f6",
    );

    #[test]
    fn parses_complete_offline_gallery_page() {
        let page = parse(FULL_PAGE).expect("the checked-in gallery page should parse");
        assert_eq!(page.previous.as_deref(), Some("10-2"));
        assert_eq!(page.next.as_deref(), Some("20"));
        assert_eq!(page.items.len(), 2);

        let first = &page.items[0];
        assert_eq!(first.gallery_id, 123_456);
        assert_eq!(first.token, "abcdef1234");
        assert_eq!(first.title, "Title & More");
        assert_eq!(first.thumbnail_key, "sample/one.jpg");
        assert_eq!(first.category, 0x4);
        assert_eq!(first.posted, "2026-08-02 12:00");
        assert_eq!(first.uploader.as_deref(), Some("Alice & Bob"));
        assert!(!first.disowned);
        assert!((first.rating - 2.5).abs() < f32::EPSILON);
        assert!(first.rated);
        assert_eq!(first.tags, ["language:english", "artist:sample"]);
        assert_eq!(first.page_count, 12);
        assert_eq!((first.thumbnail_width, first.thumbnail_height), (150, 200));
        assert_eq!(first.favorite_slot, 0);
        assert_eq!(first.favorite_name.as_deref(), Some("Read later"));
        assert_eq!(first.favorite_note.as_deref(), Some("Keep × 2"));

        let second = &page.items[1];
        assert_eq!(second.gallery_id, 654_321);
        assert_eq!(second.category, 0x800);
        assert!((second.rating + 1.0).abs() < f32::EPSILON);
        assert!(!second.rated);
        assert!(second.disowned);
        assert_eq!(second.page_count, 0);
        assert_eq!((second.thumbnail_width, second.thumbnail_height), (0, 0));
        assert_eq!(second.favorite_slot, -2);
    }

    #[test]
    fn returns_typed_empty_and_warning_errors() {
        assert_eq!(
            parse("<!doctype html><html><body><div class='itg'></div></body></html>"),
            Err(RustError::NoHits)
        );
        assert_eq!(
            parse(
                "<!doctype html><html><body><p class='searchwarn'>You do not have any watched tags</p></body></html>"
            ),
            Err(RustError::NoWatched)
        );
        assert_eq!(
            parse(
                "<!doctype html><html><body><p class='searchwarn'>Invalid query</p></body></html>"
            ),
            Err(RustError::Error("Invalid query".to_owned()))
        );
        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn wraps_success_and_error_in_the_versioned_envelope() {
        let success = parse_envelope(FULL_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert!(success.is_ok());
        assert_eq!(
            success.payload.as_ref().map(|page| page.items.len()),
            Some(2)
        );

        let failure = parse_envelope("blocked");
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert!(!failure.is_ok());
        assert_eq!(
            failure.error,
            Some(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            GalleryPage {
                previous: Some("10-2".to_owned()),
                next: Some("20".to_owned()),
                items: vec![complete_contract_item()],
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<GalleryPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn parses_supported_gallery_keys_and_rejects_invalid_urls() {
        assert_eq!(
            gallery_key("https://e-hentai.org/g/42/0123456789/"),
            Some((42, "0123456789".to_owned()))
        );
        assert_eq!(
            gallery_key("http://exhentai.org/mpv/7/abcdef1234"),
            Some((7, "abcdef1234".to_owned()))
        );
        assert_eq!(
            gallery_key("https://e-hentai.org/lofi/g/8/aaaabbbbcc"),
            Some((8, "aaaabbbbcc".to_owned()))
        );
        assert_eq!(gallery_key("https://example.test/g/42/0123456789"), None);
    }

    #[test]
    fn normalizes_thumbnail_hosts_and_all_category_aliases() {
        assert_eq!(thumbnail_key("https://ehgt.org/t/a/b.jpg"), "a/b.jpg");
        assert_eq!(thumbnail_key("https://s.exhentai.org/t/c.jpg"), "c.jpg");
        assert_eq!(thumbnail_key("relative.jpg"), "relative.jpg");

        let cases = [
            ("misc", 0x1),
            ("doujinshi", 0x2),
            ("manga", 0x4),
            ("artistcg", 0x8),
            ("artist cg sets", 0x8),
            ("artist cg", 0x8),
            ("gamecg", 0x10),
            ("game cg sets", 0x10),
            ("game cg", 0x10),
            ("imageset", 0x20),
            ("image sets", 0x20),
            ("image set", 0x20),
            ("cosplay", 0x40),
            ("asianporn", 0x80),
            ("asian porn", 0x80),
            ("non-h", 0x100),
            ("western", 0x200),
            ("private", 0x400),
            ("unknown", 0x800),
        ];
        for (category, expected) in cases {
            assert_eq!(category_value(category), expected);
        }
    }

    #[test]
    fn parses_rating_and_thumbnail_style_boundaries() {
        assert!((parse_rating("32px 0px") - 3.0).abs() < f32::EPSILON);
        assert!((parse_rating("32px 21px") - 2.5).abs() < f32::EPSILON);
        assert!((parse_rating("unknown") + 1.0).abs() < f32::EPSILON);
        assert_eq!(
            parse_thumbnail_resolution("height:200px;width:150px"),
            (200, 150)
        );
        assert_eq!(parse_thumbnail_resolution("unknown"), (0, 0));
    }

    fn complete_contract_item() -> GalleryItemSummary {
        GalleryItemSummary {
            gallery_id: 42,
            title: "Title".to_owned(),
            token: "0123456789".to_owned(),
            title_japanese: Some("日本語".to_owned()),
            thumbnail_key: "thumb.jpg".to_owned(),
            category: 4,
            posted: "2026-08-02".to_owned(),
            uploader: Some("uploader".to_owned()),
            disowned: true,
            rating: 4.5,
            rated: true,
            tags: vec!["tag".to_owned()],
            page_count: 12,
            thumbnail_width: 150,
            thumbnail_height: 200,
            language: Some("Japanese".to_owned()),
            favorite_slot: 0,
            favorite_name: Some("Favorites".to_owned()),
            favorite_note: Some("note".to_owned()),
        }
    }
}
