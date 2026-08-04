//! Gallery detail and reader-page navigation URL interpretation.
//!
//! This module owns the legacy-compatible URL grammar for detail and single-page
//! gallery navigation. It accepts no Android objects and performs no navigation;
//! Android retains deep-link dispatch, Compose destinations, rollout selection,
//! and the frozen Kotlin parser used for rollback. Input URLs and tokens are never
//! included in errors or logs.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Current gallery-navigation parser protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android navigation adapter.
pub const MODULE: &str = "galleryNavigation";

const MAX_INPUT_BYTES: usize = 16 * 1024;

/// Closed navigation grammar selected by Android before crossing JNI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryNavigationTarget {
    /// Gallery detail or multi-page-viewer URL containing a gallery ID and token.
    Detail,
    /// Single reader-page URL containing a page token, gallery ID, and page number.
    Page,
}

/// Versioned request consumed by the gallery-navigation parser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryNavigationRequest {
    /// Protocol version; callers must use [`API_VERSION`].
    pub version: u8,
    /// Detail or single-page grammar to apply.
    pub target: GalleryNavigationTarget,
    /// Raw candidate URL or loose navigation text. It is never logged or returned.
    pub input: String,
    /// Whether to require the legacy scheme/host/path prefix.
    pub strict: bool,
}

/// Closed result shape returned by the navigation parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryNavigationKind {
    /// The selected grammar did not match the input.
    None,
    /// A gallery detail or multi-page-viewer target matched.
    Detail,
    /// A single reader-page target matched.
    Page,
}

/// Parsed navigation payload with a shape determined by [`GalleryNavigationKind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryNavigationPlan {
    /// Result discriminator controlling the nullable fields below.
    pub kind: GalleryNavigationKind,
    /// Signed gallery identifier, or `None` when [`Self::kind`] is `None`.
    pub gid: Option<i64>,
    /// Ten-character lowercase hexadecimal gallery/page token, or `None` for no match.
    pub token: Option<String>,
    /// Zero-based reader page index for `Page`; absent for `None` and `Detail`.
    pub page_index: Option<i32>,
}

impl GalleryNavigationPlan {
    fn no_match() -> Self {
        Self {
            kind: GalleryNavigationKind::None,
            gid: None,
            token: None,
            page_index: None,
        }
    }

    fn detail(gid: i64, token: &str) -> Self {
        Self {
            kind: GalleryNavigationKind::Detail,
            gid: Some(gid),
            token: Some(token.to_owned()),
            page_index: None,
        }
    }

    fn page(gid: i64, token: &str, page_index: i32) -> Self {
        Self {
            kind: GalleryNavigationKind::Page,
            gid: Some(gid),
            token: Some(token.to_owned()),
            page_index: Some(page_index),
        }
    }
}

/// Parses one request into a versioned success/error envelope.
#[must_use]
pub fn parse_envelope(request: &GalleryNavigationRequest) -> ApiEnvelope<GalleryNavigationPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error("unsupported gallery-navigation protocol version".to_owned()),
        );
    }
    match parse(request) {
        Ok(plan) => ApiEnvelope::ok(API_VERSION, MODULE, plan),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Interprets one candidate using the selected legacy-compatible navigation grammar.
///
/// # Errors
///
/// Returns [`RustError::Error`] when the UTF-8 request exceeds the defensive input
/// bound. The error never includes the URL or token.
pub fn parse(request: &GalleryNavigationRequest) -> Result<GalleryNavigationPlan, RustError> {
    if request.input.len() > MAX_INPUT_BYTES {
        return Err(RustError::Error(
            "gallery-navigation input exceeds the supported length".to_owned(),
        ));
    }
    Ok(match request.target {
        GalleryNavigationTarget::Detail => parse_detail(&request.input, request.strict),
        GalleryNavigationTarget::Page => parse_page(&request.input, request.strict),
    })
}

fn parse_detail(input: &str, strict: bool) -> GalleryNavigationPlan {
    let pattern = if strict {
        regex!(
            r"https?://(?:exhentai\.org|e-hentai\.org(?:/lofi)?)/(?:g|mpv)/([0-9]+)/([0-9a-f]{10})"
        )
    } else {
        regex!(r"([0-9]+)/([0-9a-f]{10})(?:[^0-9a-f]|$)")
    };
    let Some(captures) = pattern.captures(input) else {
        return GalleryNavigationPlan::no_match();
    };
    let Some(gid) = capture_i64(&captures, 1) else {
        return GalleryNavigationPlan::no_match();
    };
    let Some(token) = captures.get(2).map(|value| value.as_str()) else {
        return GalleryNavigationPlan::no_match();
    };
    GalleryNavigationPlan::detail(gid, token)
}

fn parse_page(input: &str, strict: bool) -> GalleryNavigationPlan {
    let pattern = if strict {
        regex!(
            r"https?://(?:exhentai\.org|e-hentai\.org(?:/lofi)?)/s/([0-9a-f]{10})/([0-9]+)-([0-9]+)"
        )
    } else {
        regex!(r"([0-9a-f]{10})/([0-9]+)-([0-9]+)")
    };
    let Some(captures) = pattern.captures(input) else {
        return GalleryNavigationPlan::no_match();
    };
    let Some(token) = captures.get(1).map(|value| value.as_str()) else {
        return GalleryNavigationPlan::no_match();
    };
    let Some(gid) = capture_i64(&captures, 2) else {
        return GalleryNavigationPlan::no_match();
    };
    let Some(page_index) = capture_i32(&captures, 3).and_then(|page| page.checked_sub(1)) else {
        return GalleryNavigationPlan::no_match();
    };
    GalleryNavigationPlan::page(gid, token, page_index)
}

fn capture_i64(captures: &regex_lite::Captures<'_>, index: usize) -> Option<i64> {
    captures.get(index)?.as_str().parse().ok()
}

fn capture_i32(captures: &regex_lite::Captures<'_>, index: usize) -> Option<i32> {
    captures.get(index)?.as_str().parse().ok()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::RustError;
    use serde_json::json;

    use super::{
        API_VERSION, GalleryNavigationKind, GalleryNavigationPlan, GalleryNavigationRequest,
        GalleryNavigationTarget, MAX_INPUT_BYTES, MODULE, parse, parse_envelope,
    };

    #[test]
    fn parses_strict_detail_and_multi_page_urls_for_supported_hosts() {
        for (input, gid, token) in [
            ("https://e-hentai.org/g/42/0123456789", 42, "0123456789"),
            ("http://exhentai.org/mpv/7/abcdef0123", 7, "abcdef0123"),
            (
                "prefix https://e-hentai.org/lofi/g/9/aaaaaaaaaa?ignored=1 suffix",
                9,
                "aaaaaaaaaa",
            ),
        ] {
            assert_eq!(
                parse(&request(GalleryNavigationTarget::Detail, input, true)),
                Ok(GalleryNavigationPlan::detail(gid, token))
            );
        }
    }

    #[test]
    fn parses_strict_reader_pages_with_zero_based_legacy_indices() {
        for (input, gid, token, page_index) in [
            (
                "https://e-hentai.org/s/0123456789/42-1",
                42,
                "0123456789",
                0,
            ),
            (
                "http://exhentai.org/s/abcdef0123/7-0/trailing",
                7,
                "abcdef0123",
                -1,
            ),
            (
                "prefix https://e-hentai.org/lofi/s/aaaaaaaaaa/9-2147483647 suffix",
                9,
                "aaaaaaaaaa",
                i32::MAX - 1,
            ),
        ] {
            assert_eq!(
                parse(&request(GalleryNavigationTarget::Page, input, true)),
                Ok(GalleryNavigationPlan::page(gid, token, page_index))
            );
        }
    }

    #[test]
    fn preserves_loose_search_and_detail_token_boundary_behavior() {
        assert_eq!(
            parse(&request(
                GalleryNavigationTarget::Detail,
                "prefix 00042/abcdef0123! suffix",
                false,
            )),
            Ok(GalleryNavigationPlan::detail(42, "abcdef0123"))
        );
        assert_eq!(
            parse(&request(
                GalleryNavigationTarget::Page,
                "prefix 0123456789/42-12 trailing",
                false,
            )),
            Ok(GalleryNavigationPlan::page(42, "0123456789", 11))
        );
        assert_eq!(
            parse(&request(
                GalleryNavigationTarget::Detail,
                "42/abcdef01234",
                false,
            )),
            Ok(GalleryNavigationPlan::no_match())
        );
    }

    #[test]
    fn returns_no_match_for_wrong_shapes_hosts_case_and_numeric_overflow() {
        for request in [
            request(
                GalleryNavigationTarget::Detail,
                "https://example.com/g/42/0123456789",
                true,
            ),
            request(
                GalleryNavigationTarget::Detail,
                "https://e-hentai.org/g/42/ABCDEF0123",
                true,
            ),
            request(
                GalleryNavigationTarget::Detail,
                "https://e-hentai.org/g/9223372036854775808/0123456789",
                true,
            ),
            request(
                GalleryNavigationTarget::Page,
                "https://e-hentai.org/g/42/0123456789",
                true,
            ),
            request(
                GalleryNavigationTarget::Page,
                "https://e-hentai.org/s/0123456789/42-2147483648",
                true,
            ),
        ] {
            assert_eq!(parse(&request), Ok(GalleryNavigationPlan::no_match()));
        }
    }

    #[test]
    fn enforces_the_input_bound_without_copying_input_into_errors() {
        let prefix = "https://e-hentai.org/g/42/0123456789";
        let at_limit = format!("{prefix}{}", "x".repeat(MAX_INPUT_BYTES - prefix.len()));
        assert_eq!(at_limit.len(), MAX_INPUT_BYTES);
        assert_eq!(
            parse(&request(GalleryNavigationTarget::Detail, &at_limit, true)),
            Ok(GalleryNavigationPlan::detail(42, "0123456789"))
        );

        let over_limit = format!("{at_limit}secret");
        assert_eq!(
            parse(&request(GalleryNavigationTarget::Detail, &over_limit, true)),
            Err(RustError::Error(
                "gallery-navigation input exceeds the supported length".to_owned()
            ))
        );
        let error = parse_envelope(&request(GalleryNavigationTarget::Detail, &over_limit, true))
            .error
            .expect("over-limit input should return an error");
        assert!(!format!("{error:?}").contains("secret"));
    }

    #[test]
    fn request_and_response_have_stable_json_and_cbor_contracts() {
        let request = request(
            GalleryNavigationTarget::Page,
            "https://e-hentai.org/s/0123456789/42-3",
            true,
        );
        assert_eq!(
            serde_json::to_value(&request).expect("request JSON should encode"),
            json!({
                "version": 1,
                "target": "page",
                "input": "https://e-hentai.org/s/0123456789/42-3",
                "strict": true,
            })
        );
        let request_bytes = serde_cbor::to_vec(&request).expect("request CBOR should encode");
        assert_eq!(
            to_hex(&request_bytes),
            "a46776657273696f6e0166746172676574647061676565696e707574782668747470733a2f2f652d68656e7461692e6f72672f732f303132333435363738392f34322d3366737472696374f5"
        );

        let envelope = parse_envelope(&request);
        assert_eq!(
            envelope.payload,
            Some(GalleryNavigationPlan::page(42, "0123456789", 2))
        );
        let response_bytes = serde_cbor::to_vec(&envelope).expect("response CBOR should encode");
        assert_eq!(
            to_hex(&response_bytes),
            "a46776657273696f6e01666d6f64756c657167616c6c6572794e617669676174696f6e677061796c6f6164a4646b696e64647061676563676964182a65746f6b656e6a303132333435363738396970616765496e64657802656572726f72f6"
        );
    }

    #[test]
    fn envelope_uses_stable_metadata_and_sanitized_version_failure() {
        let mut request = request(
            GalleryNavigationTarget::Detail,
            "https://e-hentai.org/g/42/0123456789",
            true,
        );
        request.version = 2;
        let envelope = parse_envelope(&request);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(
                "unsupported gallery-navigation protocol version".to_owned()
            ))
        );
        assert_eq!(envelope.payload, None);

        let no_match = parse_envelope(&GalleryNavigationRequest {
            version: API_VERSION,
            target: GalleryNavigationTarget::Detail,
            input: String::new(),
            strict: false,
        });
        assert_eq!(
            no_match.payload.map(|plan| plan.kind),
            Some(GalleryNavigationKind::None)
        );
    }

    fn request(
        target: GalleryNavigationTarget,
        input: &str,
        strict: bool,
    ) -> GalleryNavigationRequest {
        GalleryNavigationRequest {
            version: API_VERSION,
            target,
            input: input.to_owned(),
            strict,
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
