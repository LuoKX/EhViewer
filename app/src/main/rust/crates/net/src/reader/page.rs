//! Reader image-source response parsing.

use std::borrow::Cow;

use eh_domain::contracts::{ApiEnvelope, ReaderPage, RustError};

/// Current reader-page bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "readerPage";

/// Parses a reader-page response into a versioned success/error envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<ReaderPage> {
    match parse(input) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses the image URLs and retry keys embedded in one reader-page response.
///
/// Direct gallery-page HTML is attempted first. If its required image markup is
/// absent and the response contains JSON escape characters, the parser retries
/// after removing backslashes to preserve the legacy `showpage` API behavior.
///
/// # Errors
///
/// Returns [`RustError::IpBanned`] when the response contains no HTML fragment,
/// or [`RustError::Error`] when required image markup is absent.
pub fn parse(input: &str) -> Result<ReaderPage, RustError> {
    match parse_html_fragment(input) {
        Ok(page) => Ok(page),
        Err(RustError::Error(_)) if input.contains('\\') => {
            let normalized = input
                .chars()
                .filter(|character| *character != '\\')
                .collect::<String>();
            parse_html_fragment(&normalized)
        }
        Err(error) => Err(error),
    }
}

fn parse_html_fragment(input: &str) -> Result<ReaderPage, RustError> {
    if !input.contains('<') {
        return Err(RustError::IpBanned(input.to_owned()));
    }
    let image_url = regex!(r#"<img[^>]*src="([^"]+)" style"#)
        .captures(input)
        .map(|captures| decode_legacy_entities(&captures[1]))
        .ok_or_else(|| RustError::Error("reader page image URL is missing".to_owned()))?;
    let skip_hath_key = regex!(r#"onclick="return nl\('([^)]+)'\)"#)
        .captures(input)
        .map(|captures| captures[1].to_owned());
    let original_image_url = regex!(r#"<a href="([^"]+/fullimg/[^"]+)">"#)
        .captures(input)
        .map(|captures| decode_legacy_entities(&captures[1]));
    let show_key = regex!(r#"var showkey="([0-9a-z]+)";"#)
        .captures(input)
        .map(|captures| captures[1].to_owned());

    Ok(ReaderPage {
        image_url,
        skip_hath_key,
        original_image_url,
        show_key,
    })
}

fn decode_legacy_entities(raw: &str) -> String {
    const ENTITIES: [(&str, &str); 7] = [
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#039;", "'"),
        ("&nbsp;", " "),
        ("&times;", "×"),
    ];
    let mut decoded = Cow::Borrowed(raw);
    for (entity, replacement) in ENTITIES {
        if decoded.contains(entity) {
            decoded = Cow::Owned(decoded.replace(entity, replacement));
        }
    }
    decoded.into_owned()
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{ApiEnvelope, ReaderPage, RustError};

    use super::{API_VERSION, MODULE, decode_legacy_entities, parse, parse_envelope};
    use crate::test_support::to_hex;

    const FULL_PAGE: &str = include_str!("../../test_data/reader_page.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656a72656164657250616765677061796c6f6164a468696d",
        "61676555726c781c68747470733a2f2f696d672e6578616d706c652f706167652e6a70676b736b69",
        "70486174684b657964736b6970706f726967696e616c496d61676555726cf66773686f774b65796473",
        "686f77656572726f72f6",
    );

    #[test]
    fn parses_complete_and_minimal_reader_pages_from_offline_html() {
        let complete = parse(FULL_PAGE).expect("the checked-in reader page should parse");
        assert_eq!(
            complete,
            ReaderPage {
                image_url: "https://img.example/page.jpg?first=1&second=2".to_owned(),
                skip_hath_key: Some("skip-key".to_owned()),
                original_image_url: Some(
                    "https://img.example/fullimg/42/page.jpg?download=1&source=reader".to_owned()
                ),
                show_key: Some("show123".to_owned()),
            }
        );

        let minimal =
            parse("<!doctype html><html><body><img src=\"minimal.jpg\" style=\"width:1px\"></body></html>")
                .expect("optional reader keys may be absent");
        assert_eq!(minimal.image_url, "minimal.jpg");
        assert_eq!(minimal.skip_hath_key, None);
        assert_eq!(minimal.original_image_url, None);
        assert_eq!(minimal.show_key, None);
    }

    #[test]
    fn parses_backslash_escaped_showpage_api_fragments() {
        let input = r#"{"i":"<img src=\"https://img.example/api.jpg?x=1&amp;y=2\" style=\"width:1px\">","s":"var showkey=\"api123\";"}"#;
        let page = parse(input).expect("escaped API markup should preserve legacy behavior");
        assert_eq!(page.image_url, "https://img.example/api.jpg?x=1&y=2");
        assert_eq!(page.show_key.as_deref(), Some("api123"));
    }

    #[test]
    fn preserves_known_and_unknown_legacy_entities() {
        assert_eq!(
            decode_legacy_entities("&amp;&lt;&gt;&quot;&#039;&nbsp;&times;&unknown;"),
            "&<>\"' ×&unknown;"
        );
    }

    #[test]
    fn returns_typed_errors_for_non_html_and_missing_image_markup() {
        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
        assert_eq!(
            parse("<!doctype html><html><body>missing</body></html>"),
            Err(RustError::Error(
                "reader page image URL is missing".to_owned()
            ))
        );
    }

    #[test]
    fn wraps_success_error_and_matches_the_android_cbor_golden_fixture() {
        let success = parse_envelope(FULL_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert!(success.is_ok());

        let failure = parse_envelope("blocked");
        assert_eq!(
            failure.error,
            Some(RustError::IpBanned("blocked".to_owned()))
        );

        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            ReaderPage {
                image_url: "https://img.example/page.jpg".to_owned(),
                skip_hath_key: Some("skip".to_owned()),
                original_image_url: None,
                show_key: Some("show".to_owned()),
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);
        let decoded: ApiEnvelope<ReaderPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }
}
