//! Gallery multi-page-viewer token parsing.
//!
//! This module owns only the source response grammar. Android retains the MPV
//! request URL, HTTP session, cookies, token cache, page scheduling, and fallback
//! preview requests. Tokens are sensitive path capabilities and are never copied
//! into errors or logs.

use eh_domain::contracts::{ApiEnvelope, MpvTokens, RustError};
use serde_json::{Map, Value};

const IMAGE_LIST_MARKER: &str = "var imagelist = ";
const INVALID_IMAGE_LIST: &str = "MPV image list is invalid";

/// Current MPV-token parser protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "readerMpv";

/// Parses one gallery MPV response into a versioned envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<MpvTokens> {
    match parse(input) {
        Ok(tokens) => ApiEnvelope::ok(API_VERSION, MODULE, tokens),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Extracts the first MPV image-list assignment and returns its page tokens.
///
/// Source order, duplicate tokens, empty strings, and unknown item fields are
/// preserved to match the Android parser. The exact case-sensitive marker is
/// required; this deliberately rejects the legacy substring bug that could parse
/// markerless input beginning at a fixed character offset.
///
/// # Errors
///
/// Returns a sanitized [`RustError::Error`] when the marker or terminating
/// semicolon is missing, the JSON shape is invalid, or any item lacks a string
/// `k` field. Response text and tokens are never included in the error.
pub fn parse(input: &str) -> Result<MpvTokens, RustError> {
    let json_start = input
        .find(IMAGE_LIST_MARKER)
        .map(|index| index + IMAGE_LIST_MARKER.len())
        .ok_or_else(invalid_image_list)?;
    let json_end = input[json_start..]
        .find(';')
        .map(|offset| json_start + offset)
        .ok_or_else(invalid_image_list)?;
    let items: Vec<Map<String, Value>> =
        serde_json::from_str(&input[json_start..json_end]).map_err(|_| invalid_image_list())?;
    let tokens = items
        .into_iter()
        .map(|item| {
            item.get("k")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(invalid_image_list)
        })
        .collect::<Result<_, _>>()?;
    Ok(MpvTokens { tokens })
}

fn invalid_image_list() -> RustError {
    RustError::Error(INVALID_IMAGE_LIST.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{MpvTokens, RustError};

    use super::{API_VERSION, INVALID_IMAGE_LIST, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    #[test]
    fn parses_offline_mpv_fixture_in_page_order() {
        assert_eq!(
            parse(include_str!("../../test_data/reader_mpv.html"))
                .expect("offline MPV fixture should parse"),
            MpvTokens {
                tokens: vec![
                    "0123456789".to_owned(),
                    "abcdef0123".to_owned(),
                    "0123456789".to_owned(),
                ]
            }
        );
    }

    #[test]
    fn preserves_empty_lists_empty_tokens_unknown_fields_and_duplicate_keys() {
        assert_eq!(
            parse("var imagelist = [];").expect("empty image list should succeed"),
            MpvTokens { tokens: Vec::new() }
        );
        assert_eq!(
            parse(
                r#"var imagelist = [{"k":""},{"ignored":1,"k":"first","k":"last"},{"k":"last"}];"#
            )
            .expect("legacy-compatible values should parse"),
            MpvTokens {
                tokens: vec![String::new(), "last".to_owned(), "last".to_owned()]
            }
        );
    }

    #[test]
    fn uses_the_first_marker_and_first_following_semicolon() {
        let input = concat!(
            "prefix var imagelist = [{\"k\":\"first\"}];",
            "var imagelist = [{\"k\":\"second\"}];"
        );
        assert_eq!(
            parse(input).expect("first assignment should parse").tokens,
            ["first"]
        );
        assert_eq!(
            parse(r#"var imagelist = [{"k":"before;after"}];"#),
            Err(RustError::Error(INVALID_IMAGE_LIST.to_owned()))
        );
    }

    #[test]
    fn rejects_missing_boundaries_and_invalid_json_shapes_without_leaking_input() {
        let secret = "sensitive-page-token";
        for input in [
            "",
            r#"123456789012345[{"k":"legacy-offset"}];"#,
            r#"var imagelist = [{"k":"missing terminator"}]"#,
            "var imagelist = null;",
            "var imagelist = {};",
            "var imagelist = [1];",
            "var imagelist = [{}];",
            "var imagelist = [{\"k\":null}];",
            "var imagelist = [{\"k\":1}];",
            "var imagelist = [{\"k\":\"value\"},];",
            &format!("var imagelist = [{{\"k\":\"{secret}}} trailing];"),
        ] {
            let error = parse(input).expect_err("invalid MPV input should fail");
            assert_eq!(error, RustError::Error(INVALID_IMAGE_LIST.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope =
            parse_envelope(r#"var imagelist = [{"k":"0123456789"},{"k":"abcdef0123"}];"#);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope
                .payload
                .as_ref()
                .map(|payload| payload.tokens.len()),
            Some(2)
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            "a46776657273696f6e01666d6f64756c65697265616465724d7076677061796c6f6164a166746f6b656e73826a303132333435363738396a61626364656630313233656572726f72f6"
        );

        let failure = parse_envelope("missing marker");
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(INVALID_IMAGE_LIST.to_owned()))
        );
    }
}
