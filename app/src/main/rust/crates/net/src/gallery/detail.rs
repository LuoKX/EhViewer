//! Rich gallery-detail response parsing.

use eh_domain::contracts::{
    ApiEnvelope, GalleryDetail, GalleryDetailPage, GalleryItemSummary, GalleryVersionSummary,
    RustError,
};
use tl::{HTMLTag, Node, NodeHandle, Parser, ParserOptions, VDom};

use super::{comments, preview, tags};
use crate::gallery::list::{category_value, gallery_key, thumbnail_key};
use crate::html::first_element_child;
use crate::news;
use crate::text::unescape;

/// Current rich gallery-detail bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryDetail";

/// Parses rich gallery-detail HTML and always returns a versioned success/error
/// envelope suitable for the V1 JNI boundary.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<GalleryDetailPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses one rich gallery-detail response, including tags, comments, previews,
/// and optional event-pane HTML.
///
/// The HTML is parsed once. Immutable detail, tag, preview, and event data are
/// extracted first; comment-body normalization then receives exclusive mutable
/// access to the same DOM.
///
/// # Errors
///
/// Returns [`RustError::GalleryUnavailable`] for the remote unavailable panel,
/// [`RustError::IpBanned`] when the response is not HTML, and
/// [`RustError::Error`] for malformed required detail, tag, comment, or preview
/// markup.
pub fn parse(html: &str) -> Result<GalleryDetailPage, RustError> {
    let options = ParserOptions::default().track_ids();
    let mut dom = match tl::parse(html, options) {
        Ok(dom) => dom,
        Err(error) => {
            return Err(RustError::Error(format!(
                "invalid gallery-detail HTML: {error}"
            )));
        }
    };
    if dom.version().is_none() {
        return Err(RustError::IpBanned(html.to_owned()));
    }

    let (detail, tag_groups, previews, event_html) = {
        let parser = dom.parser();
        // Preview parsing owns the remote unavailable-panel classification.
        // Run it before required detail metadata so a removed gallery retains
        // its typed error even when the response omits normal detail markup.
        let previews = preview::parse_dom(&dom, parser)?;
        let detail = parse_metadata(&dom, parser, html)?;
        let tag_groups = tags::parse_detail(&dom, parser)?;
        let event_html = news::event_html(&dom, parser);
        (detail, tag_groups, previews, event_html)
    };
    if detail.gallery_info.page_count != previews.total {
        return Err(RustError::Error(
            "gallery detail and preview page counts do not match".to_owned(),
        ));
    }
    let comments = comments::parse_dom(&mut dom)?;

    Ok(GalleryDetailPage {
        detail,
        tag_groups,
        comments,
        previews,
        event_html,
    })
}

fn parse_metadata(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
    html: &str,
) -> Result<GalleryDetail, RustError> {
    let script = regex!(
        r#"var gid = (\d+);\s*var token = "([a-f0-9]+)";\s*var apiuid = ([-\d]+);\s*var apikey = "([a-f0-9]+)";\s*var average_rating = ([.\d]+);"#,
    )
    .captures(html)
    .ok_or_else(|| RustError::Error("gallery detail script metadata is malformed".to_owned()))?;
    let mut detail = GalleryDetail {
        gallery_info: empty_gallery_info(),
        api_uid: parse_number(&script[3], "gallery API user identifier")?,
        api_key: Some(script[4].to_owned()),
        torrent_count: parse_torrent_count(dom, parser)?,
        parent: None,
        newer_versions: Vec::new(),
        visible: None,
        language: None,
        size: None,
        favorite_count: 0,
        rating_count: 0,
    };
    detail.gallery_info.gallery_id = parse_number(&script[1], "gallery identifier")?;
    script[2].clone_into(&mut detail.gallery_info.token);
    detail.gallery_info.rating = parse_number(&script[5], "gallery rating")?;
    detail.gallery_info.thumbnail_key = parse_thumbnail(dom, parser)?;
    parse_titles_category_and_uploader(&mut detail, dom, parser)?;
    parse_detail_rows(&mut detail, dom, parser)?;
    detail.rating_count = parse_number(
        &required_text(dom, parser, "rating_count", "gallery rating count")?,
        "gallery rating count",
    )?;
    parse_favorite(&mut detail, dom, parser)?;
    detail.newer_versions = parse_newer_versions(dom, parser);
    Ok(detail)
}

fn parse_torrent_count(dom: &VDom<'_>, parser: &Parser<'_>) -> Result<i32, RustError> {
    let controls = required_tag(dom, parser, "gd5", "gallery download controls")?;
    let torrent = controls
        .query_selector(parser, "a[onclick]")
        .and_then(|mut matches| matches.nth(1))
        .and_then(|handle| handle.get(parser))
        .ok_or(RustError::Error(
            "gallery torrent control is missing".to_owned(),
        ))?;
    let text = torrent.inner_text(parser);
    let captures = regex!(r"[^(]+\(([^)]+)")
        .captures(&text)
        .ok_or(RustError::Error(
            "gallery torrent count is malformed".to_owned(),
        ))?;
    parse_number(&captures[1], "gallery torrent count")
}

fn parse_thumbnail(dom: &VDom<'_>, parser: &Parser<'_>) -> Result<String, RustError> {
    let container = required_tag(dom, parser, "gd1", "gallery thumbnail container")?;
    let style = first_element_child(container, parser)
        .and_then(|thumbnail| {
            thumbnail
                .attributes()
                .get("style")
                .flatten()
                .and_then(|value| value.try_as_utf8_str())
        })
        .ok_or(RustError::Error(
            "gallery thumbnail style is missing".to_owned(),
        ))?;
    let captures = regex!(r"[^(]+\(([^)]+)")
        .captures(style)
        .ok_or(RustError::Error(
            "gallery thumbnail style is malformed".to_owned(),
        ))?;
    Ok(thumbnail_key(&captures[1]))
}

fn parse_titles_category_and_uploader(
    detail: &mut GalleryDetail,
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<(), RustError> {
    let title = required_text(dom, parser, "gn", "gallery title")?;
    detail.gallery_info.title = match unescape(&title) {
        Ok(title) => title.into_owned(),
        Err(_) => {
            return Err(RustError::Error(
                "gallery title contains an invalid entity".to_owned(),
            ));
        }
    };
    let japanese = required_text(dom, parser, "gj", "gallery Japanese title")?;
    detail.gallery_info.title_japanese = unescape(&japanese).ok().map(std::borrow::Cow::into_owned);
    let category = required_text(dom, parser, "gdc", "gallery category")?;
    detail.gallery_info.category = category_value(&category.to_lowercase());

    if let Some(uploader_node) = dom
        .get_element_by_id("gdn")
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
    {
        detail.gallery_info.disowned = uploader_node
            .attributes()
            .get("style")
            .flatten()
            .and_then(|value| value.try_as_utf8_str())
            .is_some_and(|style| style.contains("opacity:0.5"));
        let raw = if let Some(child) = first_element_child(uploader_node, parser) {
            child.inner_text(parser)
        } else {
            uploader_node.inner_text(parser)
        };
        detail.gallery_info.uploader = unescape(&raw).ok().map(std::borrow::Cow::into_owned);
    }
    Ok(())
}

fn parse_detail_rows(
    detail: &mut GalleryDetail,
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<(), RustError> {
    let table = required_tag(dom, parser, "gdd", "gallery detail table")?;
    let rows = table.query_selector(parser, "tr").ok_or(RustError::Error(
        "gallery detail row query failed".to_owned(),
    ))?;
    for row in rows {
        apply_detail_row(detail, row, parser);
    }
    Ok(())
}

fn apply_detail_row(detail: &mut GalleryDetail, row: NodeHandle, parser: &Parser<'_>) {
    let Some(children) = row.get(parser).and_then(Node::children) else {
        return;
    };
    let mut fields = children
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser).and_then(Node::as_tag));
    let (Some(key), Some(value_tag)) = (fields.next(), fields.next()) else {
        return;
    };
    let Some(value) = value_tag
        .children()
        .top()
        .as_slice()
        .first()
        .and_then(|handle| handle.get(parser))
        .map(|node| node.inner_text(parser))
    else {
        return;
    };

    match key.inner_text(parser).as_ref() {
        "Posted:" => detail.gallery_info.posted = value.into_owned(),
        "Parent:" => {
            detail.parent = first_element_child(value_tag, parser).and_then(|parent| {
                parent
                    .attributes()
                    .get("href")
                    .flatten()
                    .and_then(|href| href.try_as_utf8_str())
                    .map(str::to_owned)
            });
        }
        "Visible:" => detail.visible = Some(value.into_owned()),
        "Language:" => {
            detail.language = value.strip_suffix(" &nbsp;").map(str::to_owned);
        }
        "File Size:" => detail.size = Some(value.into_owned()),
        "Length:" => {
            if let Some(count) = value
                .strip_suffix(" pages")
                .and_then(|count| count.parse().ok())
            {
                detail.gallery_info.page_count = count;
            }
        }
        "Favorited:" => {
            detail.favorite_count = match value.as_ref() {
                "Never" => 0,
                "Once" => 1,
                other => other
                    .strip_suffix(" times")
                    .and_then(|count| count.parse().ok())
                    .unwrap_or(detail.favorite_count),
            };
        }
        _ => {}
    }
}

fn parse_favorite(
    detail: &mut GalleryDetail,
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<(), RustError> {
    let favorite = required_tag(dom, parser, "fav", "gallery favorite control")?;
    let Some(slot) = first_element_child(favorite, parser) else {
        return Ok(());
    };
    detail.gallery_info.favorite_name = slot
        .attributes()
        .get("title")
        .flatten()
        .and_then(|value| value.try_as_utf8_str())
        .and_then(|value| unescape(value).ok())
        .map(std::borrow::Cow::into_owned);
    let Some(style) = slot
        .attributes()
        .get("style")
        .flatten()
        .and_then(|value| value.try_as_utf8_str())
    else {
        return Ok(());
    };
    if let Some(slot) = parse_favorite_slot(style)? {
        detail.gallery_info.favorite_slot = slot;
    }
    Ok(())
}

fn parse_favorite_slot(style: &str) -> Result<Option<i32>, RustError> {
    let Some(captures) = regex!(r"background-position:\s*\d+px\s*-\s*(\d+)px").captures(style)
    else {
        return Ok(None);
    };
    let position: i32 = parse_number(&captures[1], "gallery favorite slot")?;
    if position < 2 {
        return Err(RustError::Error(
            "gallery favorite slot is malformed".to_owned(),
        ));
    }
    Ok(Some((position - 2) / 19))
}

fn parse_newer_versions(dom: &VDom<'_>, parser: &Parser<'_>) -> Vec<GalleryVersionSummary> {
    let Some(node) = dom
        .get_element_by_id("gnd")
        .and_then(|handle| handle.get(parser))
    else {
        return Vec::new();
    };
    regex!(r#"<a href="([^"]+)">([^<]+)</a>, added ([^<]+)<"#)
        .captures_iter(&node.inner_html(parser))
        .filter_map(|captures| {
            let (gallery_id, token) = gallery_key(&captures[1])?;
            Some(GalleryVersionSummary {
                gallery_id,
                token,
                title: unescape(&captures[2]).ok()?.into_owned(),
                posted: captures[3].to_owned(),
            })
        })
        .collect()
}

fn required_tag<'a>(
    dom: &'a VDom<'_>,
    parser: &'a Parser<'_>,
    element_id: &str,
    description: &str,
) -> Result<&'a HTMLTag<'a>, RustError> {
    dom.get_element_by_id(element_id)
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
        .ok_or_else(|| RustError::Error(format!("{description} is missing")))
}

fn required_text<'a>(
    dom: &'a VDom<'_>,
    parser: &'a Parser<'_>,
    element_id: &str,
    description: &str,
) -> Result<std::borrow::Cow<'a, str>, RustError> {
    dom.get_element_by_id(element_id)
        .and_then(|handle| handle.get(parser))
        .map(|node| node.inner_text(parser))
        .ok_or_else(|| RustError::Error(format!("{description} is missing")))
}

fn parse_number<T>(value: &str, description: &str) -> Result<T, RustError>
where
    T: std::str::FromStr,
{
    match value.parse() {
        Ok(value) => Ok(value),
        Err(_) => Err(RustError::Error(format!("{description} is malformed"))),
    }
}

fn empty_gallery_info() -> GalleryItemSummary {
    GalleryItemSummary {
        gallery_id: 0,
        title: String::new(),
        token: String::new(),
        title_japanese: None,
        thumbnail_key: String::new(),
        category: 0,
        posted: String::new(),
        uploader: None,
        disowned: false,
        rating: 0.0,
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

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{
        ApiEnvelope, GalleryCommentsPage, GalleryDetail, GalleryDetailPage, GalleryItemSummary,
        GalleryPreview, GalleryPreviewPage, GalleryTagPower, GalleryTagVote, RustError,
    };

    use super::{API_VERSION, MODULE, parse, parse_envelope, parse_favorite_slot};
    use crate::test_support::to_hex;

    const FULL_PAGE: &str = include_str!("../../test_data/gallery_detail.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656d67616c6c65727944657461696c677061796c6f6164a5",
        "6664657461696cab6b67616c6c657279496e666fb363676964182a657469746c65655469746c6565",
        "746f6b656e6a30313233343536373839687469746c654a706ef6687468756d624b6579697468756d",
        "622e6a70676863617465676f72790466706f73746564606875706c6f61646572f6686469736f776e",
        "6564f466726174696e67f94480657261746564f46a73696d706c655461677380657061676573016a",
        "7468756d625769647468006b7468756d62486569676874006e73696d706c654c616e6775616765f6",
        "6c6661766f72697465536c6f74216c6661766f726974654e616d65f66c6661766f726974654e6f74",
        "65f66661706955696420666170694b6579676170692d6b65796c746f7272656e74436f756e740066",
        "706172656e74f66d6e6577657256657273696f6e73806776697369626c65f6686c616e6775616765",
        "f66473697a65f66d6661766f72697465436f756e74006b726174696e67436f756e74006974616747",
        "726f7570738068636f6d6d656e7473a268636f6d6d656e747380676861734d6f7265f46870726576",
        "69657773a268707265766965777381a7646b696e646276316375726c6b707265766965772e6a7067",
        "68706f736974696f6e006670546f6b656e65746f6b656e676f666673657458f669636c6970576964",
        "7468f66a636c6970486569676874f665746f74616c01696576656e7448746d6cf6656572726f72f6",
    );

    #[test]
    fn parses_complete_rich_gallery_detail_from_offline_html() {
        let page = parse(FULL_PAGE).expect("the checked-in gallery detail should parse");
        let detail = &page.detail;
        assert_eq!(detail.gallery_info.gallery_id, 42);
        assert_eq!(detail.gallery_info.token, "0123456789");
        assert_eq!(detail.gallery_info.title, "Title & More");
        assert_eq!(
            detail.gallery_info.title_japanese.as_deref(),
            Some("日本語")
        );
        assert_eq!(detail.gallery_info.thumbnail_key, "sample/thumb.jpg");
        assert_eq!(detail.gallery_info.category, 4);
        assert_eq!(detail.gallery_info.posted, "2026-08-02 12:00");
        assert_eq!(detail.gallery_info.uploader.as_deref(), Some("Alice & Bob"));
        assert!(detail.gallery_info.disowned);
        assert!((detail.gallery_info.rating - 4.5).abs() < f32::EPSILON);
        assert_eq!(detail.gallery_info.page_count, 12);
        assert_eq!(detail.gallery_info.favorite_slot, 0);
        assert_eq!(
            detail.gallery_info.favorite_name.as_deref(),
            Some("Read & Later")
        );
        assert_eq!(detail.api_uid, -1);
        assert_eq!(detail.api_key.as_deref(), Some("abcdef1234"));
        assert_eq!(detail.torrent_count, 3);
        assert_eq!(
            detail.parent.as_deref(),
            Some("https://e-hentai.org/g/41/parent0001/")
        );
        assert_eq!(detail.visible.as_deref(), Some("Yes"));
        assert_eq!(detail.language.as_deref(), Some("English"));
        assert_eq!(detail.size.as_deref(), Some("12 MiB"));
        assert_eq!(detail.favorite_count, 3);
        assert_eq!(detail.rating_count, 10);
        assert_eq!(detail.newer_versions.len(), 1);
        assert_eq!(detail.newer_versions[0].gallery_id, 43);
        assert_eq!(detail.newer_versions[0].title, "New & Version");

        assert_eq!(page.tag_groups.len(), 2);
        assert_eq!(page.tag_groups[0].namespace, "artist");
        assert_eq!(page.tag_groups[0].tags[0].text, "Alice & Bob");
        assert_eq!(page.tag_groups[0].tags[0].power, GalleryTagPower::Active);
        assert_eq!(page.tag_groups[0].tags[0].vote, GalleryTagVote::Up);
        assert_eq!(page.tag_groups[0].tags[1].power, GalleryTagPower::Weak);
        assert_eq!(page.tag_groups[1].tags[0].power, GalleryTagPower::Solid);
        assert_eq!(page.tag_groups[1].tags[0].vote, GalleryTagVote::Down);
        assert_eq!(page.comments.comments.len(), 1);
        assert_eq!(page.previews.previews.len(), 2);
        assert_eq!(page.previews.total, 12);
        assert_eq!(
            page.event_html.as_deref(),
            Some("<strong>Event &amp; reward</strong>")
        );
    }

    #[test]
    fn preserves_optional_detail_boundaries() {
        let html = FULL_PAGE
            .replace("<div id=\"eventpane\"><strong>Event &amp; reward</strong></div>", "")
            .replace("<div id=\"gdn\" style=\"opacity:0.5\"><a>Alice &amp; Bob</a></div>", "")
            .replace(
                "<div id=\"fav\">\n      <div title=\"Read &amp; Later\" style=\"background-position:0px -2px\"></div>\n    </div>",
                "<div id=\"fav\"></div>",
            )
            .replace(
                "<div id=\"gnd\">\n      <a href=\"https://e-hentai.org/g/43/abcdef1234/\">New &amp; Version</a>, added 2026-08-03<br>\n    </div>",
                "",
            );
        let page = parse(&html).expect("optional detail fields may be absent");
        assert_eq!(page.event_html, None);
        assert_eq!(page.detail.gallery_info.uploader, None);
        assert!(!page.detail.gallery_info.disowned);
        assert_eq!(page.detail.gallery_info.favorite_slot, -2);
        assert_eq!(page.detail.gallery_info.favorite_name, None);
        assert!(page.detail.newer_versions.is_empty());
    }

    #[test]
    fn rejects_missing_or_malformed_required_detail_markup() {
        let cases = [
            ("var gid = 42;", "script metadata"),
            ("id=\"gd5\"", "download controls"),
            ("id=\"gd1\"", "thumbnail container"),
            ("id=\"gn\"", "gallery title"),
            ("id=\"gj\"", "Japanese title"),
            ("id=\"gdc\"", "gallery category"),
            ("id=\"gdd\"", "detail table"),
            ("id=\"rating_count\"", "rating count"),
            ("id=\"fav\"", "favorite control"),
            ("id=\"taglist\"", "tag table"),
        ];
        for (needle, expected) in cases {
            let html = FULL_PAGE.replacen(needle, "removed=\"true\"", 1);
            let error = parse(&html).expect_err("required detail markup must be rejected");
            assert!(error.to_string().contains(expected), "{error}");
        }

        let bad_tag = FULL_PAGE.replacen("class=\"gtl\"", "class=\"unknown\"", 1);
        assert!(
            parse(&bad_tag)
                .expect_err("unknown tag power must be rejected")
                .to_string()
                .contains("tag power")
        );
        assert!(
            parse_favorite_slot("background-position:0px -1px")
                .expect_err("favorite arithmetic underflow must be contained")
                .to_string()
                .contains("favorite slot")
        );
        assert_eq!(
            parse_favorite_slot("background-position:0px -2px")
                .expect("valid favorite slot should parse"),
            Some(0)
        );
        let mismatched_pages = FULL_PAGE.replacen(
            "<tr><td class=\"gdt1\">Length:</td><td class=\"gdt2\">12 pages</td></tr>",
            "<tr><td class=\"gdt1\">Length:</td><td class=\"gdt2\">12 pages</td></tr>\n      <tr><td class=\"gdt1\">Length:</td><td class=\"gdt2\">11 pages</td></tr>",
            1,
        );
        assert!(
            parse(&mismatched_pages)
                .expect_err("detail and preview page counts must agree")
                .to_string()
                .contains("do not match")
        );
        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
        assert_eq!(
            parse(
                "<!doctype html><html><body><div class='d'>Gallery removed by uploader</div></body></html>"
            ),
            Err(RustError::GalleryUnavailable(
                "Gallery removed by uploader".to_owned()
            ))
        );
    }

    #[test]
    fn wraps_success_and_error_in_the_versioned_envelope() {
        let success = parse_envelope(FULL_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(
            success
                .payload
                .as_ref()
                .map(|page| page.detail.gallery_info.gallery_id),
            Some(42)
        );
        assert!(success.is_ok());

        let failure = parse_envelope("blocked");
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(
            failure.error,
            Some(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(API_VERSION, MODULE, minimal_contract_page());
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<GalleryDetailPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    fn minimal_contract_page() -> GalleryDetailPage {
        GalleryDetailPage {
            detail: GalleryDetail {
                gallery_info: GalleryItemSummary {
                    gallery_id: 42,
                    title: "Title".to_owned(),
                    token: "0123456789".to_owned(),
                    title_japanese: None,
                    thumbnail_key: "thumb.jpg".to_owned(),
                    category: 4,
                    posted: String::new(),
                    uploader: None,
                    disowned: false,
                    rating: 4.5,
                    rated: false,
                    tags: Vec::new(),
                    page_count: 1,
                    thumbnail_width: 0,
                    thumbnail_height: 0,
                    language: None,
                    favorite_slot: -2,
                    favorite_name: None,
                    favorite_note: None,
                },
                api_uid: -1,
                api_key: Some("api-key".to_owned()),
                torrent_count: 0,
                parent: None,
                newer_versions: Vec::new(),
                visible: None,
                language: None,
                size: None,
                favorite_count: 0,
                rating_count: 0,
            },
            tag_groups: Vec::new(),
            comments: GalleryCommentsPage::default(),
            previews: GalleryPreviewPage {
                previews: vec![GalleryPreview::v1("preview.jpg", 0, "token")],
                total: 1,
            },
            event_html: None,
        }
    }
}
