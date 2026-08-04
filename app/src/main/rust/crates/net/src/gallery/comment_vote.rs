//! Gallery-comment vote API response parsing.
//!
//! Android retains API credentials, gallery and comment identifiers, submitted
//! vote direction, HTTP/session state, comment refresh, and UI messaging. Rust
//! receives only response JSON and emits the three numeric fields already decoded
//! by the legacy Kotlin serializer.

use eh_domain::contracts::{ApiEnvelope, GalleryCommentVote, RustError};
use serde_json::{Map, Value};

use super::legacy_i64;

const INVALID_RESPONSE: &str = "Gallery comment vote response is invalid";

/// Current gallery-comment-vote bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryCommentVote";

/// Parses one gallery-comment-vote API response into a versioned envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<GalleryCommentVote> {
    match parse(input) {
        Ok(result) => ApiEnvelope::ok(API_VERSION, MODULE, result),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses the updated comment identifier, score, and vote state.
///
/// Compatibility follows the frozen Kotlin serializer: all three source fields
/// are required signed integers or quoted integer strings, unknown fields are
/// ignored, and duplicate fields use their final JSON value. No positivity,
/// vote-direction, score, or request-identity policy is imposed here.
///
/// # Errors
///
/// Returns one fixed [`RustError::Error`] for malformed JSON, a non-object root,
/// missing/null/wrong fields, or values outside the matching Kotlin integer width.
/// Response text and remote error fields are never included in the failure.
pub fn parse(input: &str) -> Result<GalleryCommentVote, RustError> {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(input) else {
        return Err(invalid_response());
    };
    let Some(id) = signed_i64(&object, "comment_id") else {
        return Err(invalid_response());
    };
    let Some(score) = signed_i32(&object, "comment_score") else {
        return Err(invalid_response());
    };
    let Some(vote) = signed_i32(&object, "comment_vote") else {
        return Err(invalid_response());
    };
    Ok(GalleryCommentVote { id, score, vote })
}

fn signed_i64(object: &Map<String, Value>, field: &str) -> Option<i64> {
    object.get(field).and_then(legacy_i64)
}

fn signed_i32(object: &Map<String, Value>, field: &str) -> Option<i32> {
    signed_i64(object, field).and_then(|value| i32::try_from(value).ok())
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryCommentVote, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_comment_vote.json");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c657267616c6c657279436f6d6d656e74566f7465",
        "677061796c6f6164a36269641b0000011f71fb04cb6573636f72652664766f746501656572",
        "726f72f6",
    );

    #[test]
    fn parses_the_offline_fixture_and_ignores_unknown_fields() {
        assert_eq!(
            parse(RESPONSE).expect("checked-in comment-vote JSON should parse"),
            GalleryCommentVote {
                id: 1_234_567_890_123,
                score: -7,
                vote: 1,
            }
        );
    }

    #[test]
    fn preserves_complete_signed_ranges_without_domain_or_identity_validation() {
        let cases = [
            (i64::MIN, i32::MIN, i32::MIN),
            (-1, 0, 0),
            (i64::MAX, i32::MAX, i32::MAX),
        ];
        for (id, score, vote) in cases {
            let input =
                format!(r#"{{"comment_id":{id},"comment_score":{score},"comment_vote":{vote}}}"#);
            assert_eq!(
                parse(&input).expect("legacy-compatible signed fields should parse"),
                GalleryCommentVote { id, score, vote }
            );
        }
    }

    #[test]
    fn accepts_quoted_numeric_fields_like_the_legacy_kotlin_serializer() {
        assert_eq!(
            parse(r#"{"comment_id":"-1","comment_score":"-7","comment_vote":"0"}"#)
                .expect("quoted legacy numeric fields should parse"),
            GalleryCommentVote {
                id: -1,
                score: -7,
                vote: 0,
            }
        );
    }

    #[test]
    fn duplicate_fields_use_the_last_value_and_unrelated_errors_are_ignored() {
        let input = r#"{
          "comment_id": 1,
          "comment_score": 2,
          "comment_vote": -1,
          "error": "ignored sensitive service text",
          "comment_id": -9,
          "comment_score": -8,
          "comment_vote": 7
        }"#;
        assert_eq!(
            parse(input).expect("last duplicate values should win"),
            GalleryCommentVote {
                id: -9,
                score: -8,
                vote: 7,
            }
        );
    }

    #[test]
    fn rejects_every_invalid_shape_with_one_body_free_error() {
        let secret = "sensitive-gallery-token";
        let inputs = [
            String::new(),
            "null".to_owned(),
            "[]".to_owned(),
            "1".to_owned(),
            "{}".to_owned(),
            r#"{"comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":1}"#.to_owned(),
            r#"{"comment_id":null,"comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":"not-an-integer","comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1.5,"comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":9223372036854775808,"comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":-9223372036854775809,"comment_score":1,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":2147483648,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":"1.5","comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":"2147483648","comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":-2147483649,"comment_vote":1}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":1,"comment_vote":2147483648}"#.to_owned(),
            r#"{"comment_id":1,"comment_score":1,"comment_vote":-2147483649}"#.to_owned(),
            format!(r#"{{"error":"{secret}"}}"#),
            r#"{"comment_id":1,"comment_score":1,"comment_vote":1"#.to_owned(),
        ];
        for input in inputs {
            let error = parse(&input).expect_err("invalid comment-vote response should fail");
            assert_eq!(error, RustError::Error(INVALID_RESPONSE.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(RESPONSE);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.payload,
            Some(GalleryCommentVote {
                id: 1_234_567_890_123,
                score: -7,
                vote: 1,
            })
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
