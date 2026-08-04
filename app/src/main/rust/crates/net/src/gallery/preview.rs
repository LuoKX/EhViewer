//! Gallery-preview and total-page response parsing.

use eh_domain::contracts::{ApiEnvelope, GalleryPreview, GalleryPreviewPage, RustError};
use tl::{Node, Parser, ParserOptions, VDom};

use crate::html::{first_by_class, handle_attr};

/// Current gallery-preview bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryPreview";

/// Parses gallery-preview HTML and always returns a versioned success/error
/// envelope suitable for the V1 JNI boundary.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<GalleryPreviewPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses preview thumbnails and the gallery's total image-page count.
///
/// # Errors
///
/// Returns [`RustError::GalleryUnavailable`] when the remote page contains its
/// gallery-unavailable panel, [`RustError::IpBanned`] when the response is not
/// HTML, and [`RustError::Error`] for missing or malformed preview/page markup.
pub fn parse(html: &str) -> Result<GalleryPreviewPage, RustError> {
    let dom = tl::parse(html, ParserOptions::default())
        .map_err(|error| RustError::Error(format!("invalid gallery-preview HTML: {error}")))?;
    if dom.version().is_none() {
        return Err(RustError::IpBanned(html.to_owned()));
    }
    parse_dom(&dom, dom.parser())
}

pub(crate) fn parse_dom(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<GalleryPreviewPage, RustError> {
    if let Some(error) = first_by_class(dom, "d") {
        let message = error.inner_text(parser).trim().to_owned();
        if !message.is_empty() {
            return Err(RustError::GalleryUnavailable(message));
        }
    }

    let preview_container = dom
        .get_element_by_id("gdt")
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
        .ok_or_else(|| RustError::Error("gallery preview container is missing".to_owned()))?;
    let preview_children = preview_container.children();
    let preview_handles = preview_children
        .top()
        .iter()
        .filter(|handle| handle.get(parser).and_then(Node::as_tag).is_some());
    let previews = preview_handles
        .map(|handle| parse_preview(handle.get(parser), parser))
        .collect::<Result<Vec<_>, _>>()?;
    if previews.is_empty() {
        return Err(RustError::Error(
            "gallery preview container has no previews".to_owned(),
        ));
    }
    let total = parse_total_pages(dom, parser)?;
    if previews.iter().any(|preview| preview.position >= total) {
        return Err(RustError::Error(
            "gallery preview position exceeds the total page count".to_owned(),
        ));
    }

    Ok(GalleryPreviewPage { previews, total })
}

fn parse_preview(
    node: Option<&Node<'_>>,
    parser: &Parser<'_>,
) -> Result<GalleryPreview, RustError> {
    let anchor = node.and_then(Node::as_tag).ok_or_else(|| {
        RustError::Error("gallery preview entry is not an anchor element".to_owned())
    })?;
    let href = anchor
        .attributes()
        .get("href")
        .flatten()
        .and_then(|value| value.try_as_utf8_str())
        .ok_or_else(|| RustError::Error("gallery preview link is missing".to_owned()))?;
    let link = regex!(r##"(?:https?://[^"#]+/s/([0-9a-f]{10})/\d+-|#page)(\d+)"##)
        .captures(href)
        .ok_or_else(|| RustError::Error("gallery preview link is malformed".to_owned()))?;
    let page_token = link.get(1).map_or("", |value| value.as_str());
    let position = link[2]
        .parse::<i32>()
        .ok()
        .and_then(|page| page.checked_sub(1))
        .filter(|position| *position >= 0)
        .ok_or_else(|| RustError::Error("gallery preview position is invalid".to_owned()))?;

    let preview = anchor
        .query_selector(parser, "div[title]")
        .and_then(|mut matches| matches.next())
        .ok_or_else(|| RustError::Error("gallery preview image is missing".to_owned()))?;
    let style = handle_attr(preview, parser, "style")
        .ok_or_else(|| RustError::Error("gallery preview style is missing".to_owned()))?;
    let style = regex!(r"\D+(\d+)\D+(\d+)[^(]+\(([^)]+)\)(?: -(\d+))?")
        .captures(style)
        .ok_or_else(|| RustError::Error("gallery preview style is malformed".to_owned()))?;
    let clip_width = parse_positive_dimension(&style[1], "width")?;
    let clip_height = parse_positive_dimension(&style[2], "height")?;
    let url = style[3].to_owned();

    if let Some(offset) = style.get(4) {
        let offset_x = offset
            .as_str()
            .parse::<i32>()
            .map_err(|_| RustError::Error("gallery preview offset is invalid".to_owned()))?;
        Ok(GalleryPreview::v2(
            url,
            position,
            page_token,
            offset_x,
            clip_width,
            clip_height,
        ))
    } else {
        Ok(GalleryPreview::v1(url, position, page_token))
    }
}

fn parse_positive_dimension(value: &str, name: &str) -> Result<i32, RustError> {
    value
        .parse::<i32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| RustError::Error(format!("gallery preview {name} is invalid")))
}

fn parse_total_pages(dom: &VDom<'_>, parser: &Parser<'_>) -> Result<i32, RustError> {
    let table = dom
        .get_element_by_id("gdd")
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
        .ok_or_else(|| RustError::Error("gallery detail table is missing".to_owned()))?;
    table
        .query_selector(parser, "td.gdt2")
        .and_then(|matches| {
            matches
                .filter_map(|handle| {
                    let text = handle.get(parser)?.inner_text(parser);
                    text.trim().strip_suffix(" pages")?.parse::<i32>().ok()
                })
                .find(|total| *total > 0)
        })
        .ok_or_else(|| RustError::Error("gallery page count is missing or invalid".to_owned()))
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{
        ApiEnvelope, GalleryPreview, GalleryPreviewKind, GalleryPreviewPage, RustError,
    };

    use super::{API_VERSION, MODULE, parse, parse_envelope, parse_preview};
    use crate::test_support::to_hex;

    const FULL_PAGE: &str = include_str!("../../test_data/gallery_preview.html");
    const UNAVAILABLE_PAGE: &str = include_str!("../../test_data/gallery_preview_unavailable.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656e67616c6c65727950726576696577677061796c6f61",
        "64a268707265766965777382a7646b696e646276316375726c781b68747470733a2f2f696d672e65",
        "78616d706c652f6f6e652e6a706768706f736974696f6e006670546f6b656e6a61626364656631",
        "323334676f666673657458f669636c69705769647468f66a636c6970486569676874f6a7646b696e",
        "646276326375726c781e68747470733a2f2f696d672e6578616d706c652f7370726974652e6a7067",
        "68706f736974696f6e016670546f6b656e60676f6666736574581069636c6970576964746818506a",
        "636c6970486569676874187865746f74616c0c656572726f72f6",
    );

    #[test]
    fn parses_standalone_and_sprite_previews_from_offline_html() {
        let page = parse(FULL_PAGE).expect("the checked-in preview page should parse");
        assert_eq!(page.total, 12);
        assert_eq!(page.previews.len(), 2);
        assert_eq!(
            page.previews[0],
            GalleryPreview::v1("https://img.example/one.jpg", 0, "abcdef1234")
        );
        assert_eq!(
            page.previews[1],
            GalleryPreview::v2("https://img.example/sprite.jpg", 1, "", 16, 80, 120)
        );
    }

    #[test]
    fn returns_typed_unavailable_and_non_html_errors() {
        assert_eq!(
            parse(UNAVAILABLE_PAGE),
            Err(RustError::GalleryUnavailable(
                "Gallery removed by uploader".to_owned()
            ))
        );
        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn ignores_an_empty_unavailable_marker() {
        let html = FULL_PAGE.replacen("<body>", "<body><div class='d'></div>", 1);
        assert_eq!(
            parse(&html).expect("empty marker should be ignored").total,
            12
        );
    }

    #[test]
    fn rejects_missing_or_malformed_preview_markup() {
        let cases = [
            (
                "<!doctype html><html><body><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "container is missing",
            ),
            (
                "<!doctype html><html><body><div id='gdt'></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "has no previews",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><div></div></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "link is missing",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='bad'><div title='x' style='width:1px;height:1px;background:url(x)'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "link is malformed",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page0'><div title='x' style='width:1px;height:1px;background:url(x)'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "position is invalid",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "image is missing",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "style is missing",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x' style='bad'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "style is malformed",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x' style='width:0px;height:1px;background:url(x)'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "width is invalid",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x' style='width:1px;height:1px;background:url(x) -999999999999999999999px 0'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "offset is invalid",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x' style='width:1px;height:1px;background:url(x)'></div></a></div></body></html>",
                "detail table is missing",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page1'><div title='x' style='width:1px;height:1px;background:url(x)'></div></a></div><div id='gdd'><td class='gdt2'>unknown</td></div></body></html>",
                "page count",
            ),
            (
                "<!doctype html><html><body><div id='gdt'><a href='#page2'><div title='x' style='width:1px;height:1px;background:url(x)'></div></a></div><div id='gdd'><td class='gdt2'>1 pages</td></div></body></html>",
                "exceeds the total",
            ),
        ];

        for (html, expected) in cases {
            let error = parse(html).expect_err("malformed preview HTML must be rejected");
            assert!(error.to_string().contains(expected), "{error}");
        }

        let dom = tl::parse("<!doctype html><html></html>", tl::ParserOptions::default())
            .expect("test HTML should parse");
        assert!(
            parse_preview(None, dom.parser())
                .expect_err("an absent entry must be rejected")
                .to_string()
                .contains("not an anchor")
        );
    }

    #[test]
    fn wraps_success_and_error_in_the_versioned_envelope() {
        let success = parse_envelope(FULL_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(success.payload.as_ref().map(|page| page.total), Some(12));
        assert!(success.is_ok());

        let failure = parse_envelope(UNAVAILABLE_PAGE);
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(
            failure.error,
            Some(RustError::GalleryUnavailable(
                "Gallery removed by uploader".to_owned()
            ))
        );
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            GalleryPreviewPage {
                previews: vec![
                    GalleryPreview::v1("https://img.example/one.jpg", 0, "abcdef1234"),
                    GalleryPreview::v2("https://img.example/sprite.jpg", 1, "", 16, 80, 120),
                ],
                total: 12,
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<GalleryPreviewPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
        assert_eq!(
            decoded.payload.expect("payload").previews[0].kind,
            GalleryPreviewKind::V1
        );
    }
}
