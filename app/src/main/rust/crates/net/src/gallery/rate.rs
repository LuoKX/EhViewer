//! Gallery-rating API response parsing.
//!
//! Android retains API credentials, the gallery identifier/token, request rating
//! conversion, HTTP/session state, and UI mutation. Rust receives only the response
//! JSON and emits the two numeric fields already consumed by Android.

use eh_domain::contracts::{ApiEnvelope, GalleryRating, RustError};
use serde_json::Value;

use super::{legacy_f64, legacy_i64};

const INVALID_RESPONSE: &str = "Gallery rating response is invalid";

/// Current gallery-rating bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryRate";

/// Parses one gallery-rating API response into a versioned envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<GalleryRating> {
    match parse(input) {
        Ok(rating) => ApiEnvelope::ok(API_VERSION, MODULE, rating),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses the updated average rating and rating count.
///
/// Compatibility follows the frozen Kotlin serializer: unknown fields are ignored,
/// duplicate fields use the final JSON value, and numeric strings are accepted.
/// The average accepts any finite value representable as `f32`, while the count
/// accepts the complete signed 32-bit range. No gallery-specific rating or count
/// range is imposed here.
///
/// # Errors
///
/// Returns a fixed [`RustError::Error`] for malformed JSON, a non-object root,
/// missing/null/wrong fields, a non-finite `f32` conversion, or a count outside the
/// signed 32-bit range. Response text is never included in the error.
pub fn parse(input: &str) -> Result<GalleryRating, RustError> {
    let Ok(value) = serde_json::from_str::<Value>(input) else {
        return Err(invalid_response());
    };
    let Some(object) = value.as_object() else {
        return Err(invalid_response());
    };
    let Some(rating) = object.get("rating_avg").and_then(legacy_f64) else {
        return Err(invalid_response());
    };
    // The frozen Kotlin decoder intentionally narrows JSON numbers to `Float`.
    #[allow(clippy::cast_possible_truncation)]
    let rating = rating as f32;
    if !rating.is_finite() {
        return Err(invalid_response());
    }
    let Some(rating_count) = object
        .get("rating_cnt")
        .and_then(legacy_i64)
        .and_then(|count| i32::try_from(count).ok())
    else {
        return Err(invalid_response());
    };
    Ok(GalleryRating {
        rating,
        rating_count,
    })
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryRating, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_rate.json");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656b67616c6c6572795261746567706179",
        "6c6f6164a266726174696e67f944c06b726174696e67436f756e74188065657272",
        "6f72f6",
    );

    #[test]
    fn parses_the_offline_fixture_and_ignores_unknown_fields() {
        assert_eq!(
            parse(RESPONSE).expect("checked-in gallery-rating JSON should parse"),
            GalleryRating {
                rating: 4.75,
                rating_count: 128,
            }
        );
    }

    #[test]
    fn preserves_finite_float_and_signed_count_boundaries_without_domain_validation() {
        let cases = [
            ("-0.0", i32::MIN, (-0.0_f32).to_bits()),
            ("-123.5", -1, (-123.5_f32).to_bits()),
            ("0", 0, 0_f32.to_bits()),
            ("5", i32::MAX, 5_f32.to_bits()),
            ("16777217", 7, 16_777_216_f32.to_bits()),
        ];
        for (source_rating, source_count, expected_bits) in cases {
            let input = format!(r#"{{"rating_avg":{source_rating},"rating_cnt":{source_count}}}"#);
            let parsed = parse(&input).expect("legacy-compatible rating should parse");
            assert_eq!(parsed.rating.to_bits(), expected_bits);
            assert_eq!(parsed.rating_count, source_count);
        }
    }

    #[test]
    fn accepts_quoted_numeric_fields_like_the_legacy_kotlin_serializer() {
        assert_eq!(
            parse(r#"{"rating_avg":"4.5","rating_cnt":"-7"}"#)
                .expect("quoted legacy numeric fields should parse"),
            GalleryRating {
                rating: 4.5,
                rating_count: -7,
            }
        );
    }

    #[test]
    fn duplicate_fields_use_the_last_value_and_unrelated_errors_are_ignored() {
        let input = r#"{"rating_avg":1.0,"rating_cnt":1,"error":"ignored secret","rating_avg":4.25,"rating_cnt":2}"#;
        assert_eq!(
            parse(input).expect("last duplicate values should win"),
            GalleryRating {
                rating: 4.25,
                rating_count: 2,
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
            "{}".to_owned(),
            r#"{"rating_avg":null,"rating_cnt":1}"#.to_owned(),
            r#"{"rating_avg":"not-a-number","rating_cnt":1}"#.to_owned(),
            r#"{"rating_avg":1e39,"rating_cnt":1}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":null}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":1.5}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":"1.5"}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":"2147483648"}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":2147483648}"#.to_owned(),
            r#"{"rating_avg":4.5,"rating_cnt":-2147483649}"#.to_owned(),
            format!(r#"{{"error":"{secret}"}}"#),
        ];
        for input in inputs {
            let error = parse(&input).expect_err("invalid gallery rating should fail");
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
            Some(GalleryRating {
                rating: 4.75,
                rating_count: 128,
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
