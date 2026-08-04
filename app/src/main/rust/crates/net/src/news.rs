//! News-page event-pane response parsing.
//!
//! This module owns only the remote `#eventpane` HTML grammar. Android retains
//! HTTP, cookies, daily scheduling, notification display, and request-only calls.

use eh_domain::contracts::{ApiEnvelope, NewsPage, RustError};
use tl::{Parser, ParserOptions, VDom};

/// Current News event-pane bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "news";

/// Parses News HTML into a versioned success or typed-error envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<NewsPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses the optional event pane from one News response.
///
/// Missing panes return `None`; an existing empty pane returns `Some("")`.
/// Nested markup and entity text remain in the DOM parser's inner-HTML form.
///
/// # Errors
///
/// Returns [`RustError::IpBanned`] with a fixed, body-free message when the
/// response is not HTML, and [`RustError::Error`] when the HTML parser rejects
/// an unsupported input size. No response content is copied into diagnostics.
pub fn parse(html: &str) -> Result<NewsPage, RustError> {
    let Ok(dom) = tl::parse(html, ParserOptions::default().track_ids()) else {
        return Err(RustError::Error("news response HTML is invalid".to_owned()));
    };
    if dom.version().is_none() {
        return Err(RustError::IpBanned("news response is not HTML".to_owned()));
    }

    Ok(NewsPage {
        event_html: event_html(&dom, dom.parser()),
    })
}

/// Extracts the shared event-pane grammar for News and Gallery Detail.
pub(crate) fn event_html(dom: &VDom<'_>, parser: &Parser<'_>) -> Option<String> {
    dom.get_element_by_id("eventpane")
        .and_then(|handle| handle.get(parser))
        .map(|node| node.inner_html(parser).into_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{ApiEnvelope, NewsPage, RustError};

    use super::{API_VERSION, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const NEWS_PAGE: &str = include_str!("../test_data/news.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c65646e657773677061796c6f6164a1696576656e744874",
        "6d6c78233c7374726f6e673e4576656e742026616d703b207265776172643c2f7374726f6e673e",
        "656572726f72f6",
    );

    #[test]
    fn parses_nested_event_markup_from_the_offline_fixture() {
        assert_eq!(
            parse(NEWS_PAGE),
            Ok(NewsPage {
                event_html: Some("<strong>Event &amp; reward</strong>".to_owned()),
            })
        );
    }

    #[test]
    fn preserves_missing_and_empty_event_panes() {
        assert_eq!(
            parse("<!doctype html><html><body></body></html>"),
            Ok(NewsPage { event_html: None })
        );
        assert_eq!(
            parse("<!doctype html><html><body><div id='eventpane'></div></body></html>"),
            Ok(NewsPage {
                event_html: Some(String::new()),
            })
        );
    }

    #[test]
    fn rejects_non_html_without_copying_the_response_into_the_error() {
        let secret_body = "session=secret-news-body";
        assert_eq!(
            parse(secret_body),
            Err(RustError::IpBanned("news response is not HTML".to_owned()))
        );

        let failure = parse_envelope(secret_body);
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(
            failure.error,
            Some(RustError::IpBanned("news response is not HTML".to_owned()))
        );
        let error = failure
            .error
            .as_ref()
            .expect("failure envelope should contain an error");
        assert!(!error.to_string().contains(secret_body));
    }

    #[test]
    fn wraps_success_and_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(NEWS_PAGE);
        assert!(envelope.is_ok());
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);

        let encoded = serde_cbor::to_vec(&envelope).expect("news envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);
        let decoded: ApiEnvelope<NewsPage> =
            serde_cbor::from_slice(&encoded).expect("news golden should decode in Rust");
        assert_eq!(decoded, envelope);
    }
}
