//! Gallery-token API response parsing.
//!
//! This module owns only the remote JSON response grammar. Android retains the
//! request gallery identifier, existing gallery token, page number, HTTP session,
//! site selection, and navigation. Returned tokens are sensitive capabilities and
//! are never copied into errors or logs.

use eh_domain::contracts::{ApiEnvelope, GalleryToken, RustError};
use serde::Deserialize;
use serde_json::Value;

const INVALID_RESPONSE: &str = "Gallery token response is invalid";
const REQUEST_FAILED: &str = "Gallery token request failed";

/// Current gallery-token bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryToken";

/// Parses one gallery-token API response into a versioned envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<GalleryToken> {
    match parse(input) {
        Ok(token) => ApiEnvelope::ok(API_VERSION, MODULE, token),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Returns the first gallery token from a legacy-compatible API response.
///
/// The complete `tokenlist` is decoded before the first entry is selected, so a
/// malformed later item still rejects the success shape. Unknown fields are
/// ignored. The source `gid` must fit in a signed 64-bit integer but is otherwise
/// ignored, matching the Android parser. Empty and opaque token strings are
/// preserved without unevidenced format checks.
///
/// # Errors
///
/// Returns a fixed [`RustError::Error`] for source-declared failures or malformed
/// JSON. Response text, service messages, and gallery tokens are never included
/// in the error.
pub fn parse(input: &str) -> Result<GalleryToken, RustError> {
    let value: Value = serde_json::from_str(input).map_err(|_| invalid_response())?;
    if !value.is_object() {
        return Err(invalid_response());
    }

    if let Ok(response) = TokenResponse::deserialize(&value)
        && let Some(item) = response.token_list.into_iter().next()
    {
        return Ok(GalleryToken { token: item.token });
    }

    Err(classify_failure(&value))
}

fn classify_failure(value: &Value) -> RustError {
    if FailureResponse::deserialize(value).is_ok() {
        RustError::Error(REQUEST_FAILED.to_owned())
    } else {
        invalid_response()
    }
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(rename = "tokenlist")]
    token_list: Vec<TokenItem>,
}

#[derive(Deserialize)]
struct TokenItem {
    #[serde(rename = "gid")]
    _gallery_id: i64,
    token: String,
}

#[derive(Deserialize)]
struct FailureResponse {
    #[serde(default, rename = "error")]
    _error: Option<String>,
    #[serde(default, rename = "tagpane")]
    _tag_pane: Option<String>,
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryToken, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, REQUEST_FAILED, parse, parse_envelope};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_token.json");
    const V1_GOLDEN_CBOR_HEX: &str = "a46776657273696f6e01666d6f64756c656c67616c6c657279546f6b656e677061796c6f6164a165746f6b656e6a30343339666133363636656572726f72f6";

    #[test]
    fn parses_the_first_token_from_the_offline_fixture() {
        assert_eq!(
            parse(RESPONSE).expect("checked-in gallery-token JSON should parse"),
            GalleryToken {
                token: "0439fa3666".to_owned()
            }
        );
    }

    #[test]
    fn preserves_opaque_empty_and_unicode_tokens_without_validating_gid() {
        let cases = [(i64::MIN, ""), (0, "opaque token"), (i64::MAX, "令牌")];
        for (gallery_id, expected) in cases {
            let input = format!(r#"{{"tokenlist":[{{"gid":{gallery_id},"token":"{expected}"}}]}}"#);
            assert_eq!(
                parse(&input)
                    .expect("legacy-compatible token should parse")
                    .token,
                expected
            );
        }
    }

    #[test]
    fn decodes_every_item_before_selecting_the_first() {
        let input = r#"{"tokenlist":[{"gid":1,"token":"first"},{"gid":"bad","token":"later"}]}"#;
        assert_eq!(
            parse(input),
            Err(RustError::Error(REQUEST_FAILED.to_owned()))
        );
    }

    #[test]
    fn maps_legacy_fallback_objects_to_a_sanitized_service_failure() {
        let secret = "sensitive-gallery-token";
        let inputs = [
            format!(r#"{{"error":"failed for {secret}"}}"#),
            "{}".to_owned(),
            r#"{"error":null}"#.to_owned(),
            r#"{"tagpane":"ignored"}"#.to_owned(),
            r#"{"tokenlist":[]}"#.to_owned(),
            r#"{"tokenlist":[{"gid":9223372036854775808,"token":"value"}]}"#.to_owned(),
            r#"{"tokenlist":[{"gid":1,"token":null}]}"#.to_owned(),
        ];

        for input in inputs {
            let error = parse(&input).expect_err("legacy fallback object should fail safely");
            assert_eq!(error, RustError::Error(REQUEST_FAILED.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn rejects_non_objects_and_invalid_fallback_fields_without_leaking_input() {
        let secret = "sensitive-gallery-token";
        let inputs = [
            String::new(),
            "null".to_owned(),
            "[]".to_owned(),
            "1".to_owned(),
            "{".to_owned(),
            format!(r#"{{"error":1,"token":"{secret}"}}"#),
            r#"{"tagpane":1}"#.to_owned(),
        ];

        for input in inputs {
            let error = parse(&input).expect_err("invalid response should fail safely");
            assert_eq!(
                error,
                RustError::Error(INVALID_RESPONSE.to_owned()),
                "{input}"
            );
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(RESPONSE);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope
                .payload
                .as_ref()
                .map(|payload| payload.token.as_str()),
            Some("0439fa3666")
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            V1_GOLDEN_CBOR_HEX
        );

        let failure = parse_envelope("not JSON");
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(INVALID_RESPONSE.to_owned()))
        );
    }
}
