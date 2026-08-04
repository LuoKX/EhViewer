//! Gallery tag-vote API response parsing.
//!
//! Android retains the API user/key, gallery identifier/token, submitted tag text,
//! vote value, HTTP session, and UI mutation. Rust receives only the response JSON,
//! parses its tag-pane fragment, and returns updated platform-neutral tag groups.

use eh_domain::contracts::{ApiEnvelope, GalleryTags, RustError};
use serde_json::Value;
use tl::ParserOptions;

use super::tags;

const INVALID_RESPONSE: &str = "Gallery tag vote response is invalid";

/// Current gallery tag-vote bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryTagVote";

/// Parses one tag-vote response into a versioned envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<GalleryTags> {
    match parse(input) {
        Ok(tags) => ApiEnvelope::ok(API_VERSION, MODULE, tags),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses updated gallery tag groups from one tag-vote API response.
///
/// A string `error` field takes precedence over `tagpane`, matching the frozen
/// parser and preserving the service's user-facing correction suggestions. A
/// missing or non-string error is ignored. The tag-pane parser preserves group and
/// tag order, legacy alias stripping, supported entities, empty tag text, and vote
/// marker requirements.
///
/// # Errors
///
/// Returns the source error string as [`RustError::Error`] when present. Malformed
/// JSON, a missing/non-string `tagpane`, malformed fragment HTML, or an invalid tag
/// row returns a fixed error that never contains response JSON or HTML.
pub fn parse(input: &str) -> Result<GalleryTags, RustError> {
    let value: Value = serde_json::from_str(input).map_err(|_| invalid_response())?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(RustError::Error(error.to_owned()));
    }
    let tag_pane = value
        .get("tagpane")
        .and_then(Value::as_str)
        .ok_or_else(invalid_response)?;
    let Ok(dom) = tl::parse(tag_pane, ParserOptions::default()) else {
        return Err(invalid_response());
    };
    let groups = tags::parse_vote_response(&dom, dom.parser()).map_err(|_| invalid_response())?;
    Ok(GalleryTags { groups })
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryTagPower, GalleryTagVote, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_tag_vote.json");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656e67616c6c657279546167566f7465",
        "677061796c6f6164a16667726f75707382a2696e616d6573706163656661727469",
        "7374647461677382a364746578746b416c696365202620426f6265706f77657266",
        "61637469766564766f7465627570a364746578746065706f776572647765616b64",
        "766f7465646e6f6e65a2696e616d657370616365686c616e677561676564746167",
        "7381a3647465787467656e676c69736865706f77657265736f6c696464766f7465",
        "64646f776e656572726f72f6",
    );

    #[test]
    fn parses_updated_groups_from_the_offline_fixture_in_source_order() {
        let tags = parse(RESPONSE).expect("checked-in tag-vote JSON should parse");
        assert_eq!(tags.groups.len(), 2);
        assert_eq!(tags.groups[0].namespace, "artist");
        assert_eq!(tags.groups[0].tags[0].text, "Alice & Bob");
        assert_eq!(tags.groups[0].tags[0].power, GalleryTagPower::Active);
        assert_eq!(tags.groups[0].tags[0].vote, GalleryTagVote::Up);
        assert_eq!(tags.groups[0].tags[1].text, "");
        assert_eq!(tags.groups[0].tags[1].power, GalleryTagPower::Weak);
        assert_eq!(tags.groups[1].namespace, "language");
        assert_eq!(tags.groups[1].tags[0].vote, GalleryTagVote::Down);
    }

    #[test]
    fn preserves_empty_results_unknown_vote_classes_and_non_string_errors() {
        for input in [
            r#"{"tagpane":"No tags have been added yet."}"#,
            r#"{"tagpane":"<table></table>"}"#,
            r#"{"error":null,"tagpane":"<table></table>"}"#,
            r#"{"error":1,"tagpane":"<table></table>"}"#,
        ] {
            assert!(
                parse(input)
                    .expect("legacy-compatible empty response should parse")
                    .groups
                    .is_empty()
            );
        }

        let tags = parse(
            r#"{"tagpane":"<table><tr><td>artist:</td><td><div class='gt'><span class='unknown'></span>tag</div></td></tr></table>"}"#,
        )
        .expect("unknown vote marker class should map to none");
        assert_eq!(tags.groups[0].tags[0].vote, GalleryTagVote::None);
    }

    #[test]
    fn source_error_string_wins_without_copying_the_remaining_response() {
        let message = "The tag is blocked; try artist:example";
        let secret = "sensitive-gallery-token";
        let input = format!(r#"{{"error":"{message}","tagpane":"{secret}","ignored":"{secret}"}}"#);
        let error = parse(&input).expect_err("source error should win");
        assert_eq!(error, RustError::Error(message.to_owned()));
        assert!(!error.to_string().contains(secret));
    }

    #[test]
    fn rejects_invalid_json_shapes_and_tag_grammar_with_one_fixed_error() {
        let secret = "sensitive-gallery-token";
        let inputs = [
            String::new(),
            "null".to_owned(),
            "[]".to_owned(),
            "{}".to_owned(),
            r#"{"tagpane":null}"#.to_owned(),
            r#"{"tagpane":1}"#.to_owned(),
            r#"{"tagpane":"<table><tr></tr></table>"}"#.to_owned(),
            r#"{"tagpane":"<table><tr><td>artist:</td><td><div class='bad'><span></span>tag</div></td></tr></table>"}"#.to_owned(),
            format!(r#"{{"tagpane":"<table><tr><td>{secret}</td></tr></table>"}}"#),
        ];
        for input in inputs {
            let error = parse(&input).expect_err("invalid tag-vote response should fail");
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
            envelope
                .payload
                .as_ref()
                .map(|payload| payload.groups.len()),
            Some(2)
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
