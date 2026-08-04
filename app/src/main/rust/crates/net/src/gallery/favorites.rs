//! Remote Favorites-page response parsing.
//!
//! This module owns only the category panel and Gallery List HTML grammar. Android
//! retains HTTP, cookies, local favorites, paging state, database enrichment, and
//! every favorite mutation. The same response parser may consume a completed batch
//! mutation response, but it never executes or replays that mutation.

use eh_domain::contracts::{ApiEnvelope, GalleryFavoritesPage, RustError};
use tl::{Parser, ParserOptions, VDom};

use super::list;
use crate::text::unescape;

const INVALID_RESPONSE: &str = "Gallery favorites response is invalid";

/// Current Gallery Favorites bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryFavorites";

/// Parses one remote Favorites response into a versioned envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<GalleryFavoritesPage> {
    match parse(html) {
        Ok(result) => ApiEnvelope::ok(API_VERSION, MODULE, result),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses category names/counts and the shared Gallery List page.
///
/// Compatibility preserves the frozen root parser's unusual category behavior:
/// `.fp` rows are filtered in source order, ordinal 10 is always skipped, names
/// are XML-unescaped then trimmed, counts use strict signed-`i32` parsing, and the
/// filtered result must contain exactly ten pairs. Missing direct children are
/// treated as an invalid response instead of repeating the legacy indexing panic.
///
/// # Errors
///
/// Returns a fixed [`RustError::Error`] when the HTML or category panel is invalid.
/// Non-HTML responses and Gallery List states preserve the existing typed errors
/// from [`list::parse`]. Source HTML is never copied into the fixed category error.
pub fn parse(html: &str) -> Result<GalleryFavoritesPage, RustError> {
    let Ok(dom) = tl::parse(html, ParserOptions::default()) else {
        return Err(invalid_response());
    };
    if dom.version().is_none() {
        return Err(RustError::IpBanned(html.to_owned()));
    }
    parse_dom(&dom, dom.parser())
}

fn parse_dom(dom: &VDom<'_>, parser: &Parser<'_>) -> Result<GalleryFavoritesPage, RustError> {
    let pairs = dom
        .get_elements_by_class_name("fp")
        .enumerate()
        .filter_map(|(index, handle)| {
            if index == 10 {
                return None;
            }
            let children = handle.get(parser)?.children()?;
            let direct = children.top();
            let name = direct.get(5)?.get(parser)?.inner_text(parser);
            let name = unescape(&name).ok()?.trim().to_owned();
            let count = direct
                .get(1)?
                .get(parser)?
                .inner_text(parser)
                .parse::<i32>()
                .ok()?;
            Some((name, count))
        })
        .collect::<Vec<_>>();
    if pairs.len() != 10 {
        return Err(invalid_response());
    }
    let (categories, counts) = pairs.into_iter().unzip();
    let page = list::parse_dom(dom, parser)?;
    Ok(GalleryFavoritesPage {
        categories,
        counts,
        page,
    })
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryFavoritesPage, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_favorites.html");
    const LIST_RESPONSE: &str = include_str!("../../test_data/gallery_list.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c657067616c6c6572794661766f7269746573677061796c6f6164a36a63617465",
        "676f726965738a6b4661766f726974657320306f4661766f72697465732026204f6e65704661766f726974657320c397",
        "2054776f6b4661766f726974657320336b4661766f726974657320346b4661766f726974657320356b4661766f726974",
        "657320366b4661766f726974657320376b4661766f726974657320386b4661766f7269746573203966636f756e74738a",
        "000102030405060708096470616765a364707265766431302d32646e6578746232306f67616c6c657279496e666f4c69",
        "737482b3636769641a0001e240657469746c656c5469746c652026204d6f726565746f6b656e6a616263646566313233",
        "34687469746c654a706ef6687468756d624b65796e73616d706c652f6f6e652e6a70676863617465676f72790466706f",
        "7374656470323032362d30382d30322031323a30306875706c6f616465726b416c696365202620426f62686469736f77",
        "6e6564f466726174696e67f94100657261746564f56a73696d706c655461677382706c616e67756167653a656e676c69",
        "73686d6172746973743a73616d706c656570616765730c6a7468756d62576964746818966b7468756d62486569676874",
        "18c86e73696d706c654c616e6775616765f66c6661766f72697465536c6f74006c6661766f726974654e616d656a5265",
        "6164206c617465726c6661766f726974654e6f7465694b65657020c3972032b3636769641a0009fbf1657469746c6567",
        "4d696e696d616c65746f6b656e6a30313233343536373839687469746c654a706ef6687468756d624b65796e73616d70",
        "6c652f74776f2e6a70676863617465676f727919080066706f73746564606875706c6f616465726a284469736f776e65",
        "6429686469736f776e6564f566726174696e67f9bc00657261746564f46a73696d706c65546167738065706167657300",
        "6a7468756d625769647468006b7468756d62486569676874006e73696d706c654c616e6775616765f66c6661766f7269",
        "7465536c6f74216c6661766f726974654e616d65f66c6661766f726974654e6f7465f6656572726f72f6",
    );

    #[test]
    fn parses_the_offline_fixture_and_reuses_the_gallery_list_contract() {
        let result = parse(RESPONSE).expect("checked-in Favorites HTML should parse");
        assert_eq!(
            result.categories,
            [
                "Favorites 0",
                "Favorites & One",
                "Favorites × Two",
                "Favorites 3",
                "Favorites 4",
                "Favorites 5",
                "Favorites 6",
                "Favorites 7",
                "Favorites 8",
                "Favorites 9",
            ]
        );
        assert_eq!(result.counts, (0..10).collect::<Vec<_>>());
        assert_eq!(result.page.previous.as_deref(), Some("10-2"));
        assert_eq!(result.page.next.as_deref(), Some("20"));
        assert_eq!(result.page.items.len(), 2);
        assert_eq!(result.page.items[0].gallery_id, 123_456);
        assert_eq!(
            result.page.items[0].favorite_name.as_deref(),
            Some("Read later")
        );
        assert_eq!(
            result.page.items[0].favorite_note.as_deref(),
            Some("Keep × 2")
        );
    }

    #[test]
    fn preserves_signed_counts_empty_names_and_the_ordinal_ten_skip() {
        let mut rows = valid_rows();
        rows[0] = category_row("   ", &i32::MIN.to_string());
        rows[9] = category_row("Maximum", &i32::MAX.to_string());
        rows.push("<div class=\"fp\">ignored malformed ordinal ten</div>".to_owned());
        let result = parse(&response(&rows, list_body()))
            .expect("complete signed counts and the skipped row should parse");
        assert_eq!(result.categories[0], "");
        assert_eq!(result.categories[9], "Maximum");
        assert_eq!(result.counts[0], i32::MIN);
        assert_eq!(result.counts[9], i32::MAX);
    }

    #[test]
    fn later_valid_rows_replace_filtered_rows_after_the_skipped_ordinal() {
        let mut rows = valid_rows();
        rows[0] = "<div class=\"fp\">malformed</div>".to_owned();
        rows.push(category_row("Always skipped", "10"));
        rows.push(category_row("Late replacement", "11"));
        let result = parse(&response(&rows, list_body()))
            .expect("legacy filtering should accept a valid row after ordinal ten");
        assert_eq!(
            result.categories.first().map(String::as_str),
            Some("Category 1")
        );
        assert_eq!(
            result.categories.last().map(String::as_str),
            Some("Late replacement")
        );
        assert_eq!(result.counts.last(), Some(&11));
    }

    #[test]
    fn rejects_invalid_category_panels_with_one_body_free_error() {
        let secret = "sensitive-favorite-note";
        let invalid_rows = [
            valid_rows()[..9].to_vec(),
            replace_row("<div class=\"fp\">missing direct children</div>"),
            replace_row(&category_row("Bad &unknown; entity", "1")),
            replace_row(&category_row("Whitespace count", " 1")),
            replace_row(&category_row("Comma count", "1,000")),
            replace_row(&category_row("Positive overflow", "2147483648")),
            replace_row(&category_row("Negative overflow", "-2147483649")),
            {
                let mut rows = valid_rows();
                rows.push(category_row("Skipped", "10"));
                rows.push(category_row("Too many", "11"));
                rows
            },
        ];
        for rows in invalid_rows {
            let html = response(&rows, &format!("{secret}{}", list_body()));
            let error = parse(&html).expect_err("invalid Favorites categories should fail");
            assert_eq!(error, RustError::Error(INVALID_RESPONSE.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn validates_categories_before_preserving_gallery_list_errors() {
        let rows = valid_rows();
        let no_hits = parse(&response(&rows, ""))
            .expect_err("valid categories without gallery rows should report no hits");
        assert_eq!(no_hits, RustError::NoHits);

        let no_watched = parse(&response(
            &rows,
            "<div class=\"searchwarn\">You do not have any watched tags</div>",
        ))
        .expect_err("watched-tag warning should retain its typed error");
        assert_eq!(no_watched, RustError::NoWatched);

        let warning = parse(&response(
            &rows,
            "<div class=\"searchwarn\">Remote favorites warning</div>",
        ))
        .expect_err("other remote warnings should retain their text");
        assert_eq!(
            warning,
            RustError::Error("Remote favorites warning".to_owned())
        );

        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(RESPONSE);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert!(matches!(
            envelope.payload,
            Some(GalleryFavoritesPage { .. })
        ));
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            V1_GOLDEN_CBOR_HEX
        );

        let failure = parse_envelope(&response(&valid_rows()[..9], list_body()));
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(INVALID_RESPONSE.to_owned()))
        );
    }

    fn valid_rows() -> Vec<String> {
        (0..10)
            .map(|index| category_row(&format!("Category {index}"), &index.to_string()))
            .collect()
    }

    fn replace_row(replacement: &str) -> Vec<String> {
        let mut rows = valid_rows();
        rows[4] = replacement.to_owned();
        rows
    }

    fn category_row(name: &str, count: &str) -> String {
        format!(
            "<div class=\"fp\"><i></i><b>{count}</b><i></i><i></i><i></i><span>{name}</span></div>"
        )
    }

    fn response(rows: &[String], body: &str) -> String {
        format!(
            "<!doctype html><html><body>{}{body}</body></html>",
            rows.join("")
        )
    }

    fn list_body() -> &'static str {
        LIST_RESPONSE
            .strip_prefix("<!doctype html>\n<html>\n  <body>\n")
            .and_then(|html| html.strip_suffix("  </body>\n</html>\n"))
            .expect("checked-in Gallery List fixture should keep its document wrapper")
    }
}
