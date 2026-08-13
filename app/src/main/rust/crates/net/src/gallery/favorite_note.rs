//! Add Favorites response parsing for the current gallery note.
//!
//! Android retains HTTP, cookies, and mutation/UI state. This module owns only the
//! frozen first-textarea response grammar and its exact legacy entity replacement.

use eh_domain::contracts::{ApiEnvelope, RustError};

use crate::text::unescape_legacy_entities;

const INVALID_RESPONSE: &str = "Favorite note response is invalid";

/// Current favorite-note response protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android parser adapter.
pub const MODULE: &str = "galleryFavoriteNote";

/// Parses one Add Favorites response into a versioned, body-free-error envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<String> {
    match parse(html) {
        Ok(note) => ApiEnvelope::ok(API_VERSION, MODULE, note),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Returns the first note matching the frozen Kotlin textarea grammar.
///
/// Matching is case-sensitive, accepts arbitrary opening-tag attributes, permits
/// empty and multiline content, performs no trimming, and decodes exactly the seven
/// entities recognized by the former Kotlin `unescapeXml` helper in one pass.
///
/// # Errors
///
/// Returns one fixed [`RustError::Error`] when no complete matching textarea exists.
/// Source HTML and note contents are never copied into the error.
pub fn parse(html: &str) -> Result<String, RustError> {
    let captures = regex!(r"<textarea[^>]*>([^<]*)</textarea>")
        .captures(html)
        .ok_or_else(invalid_response)?;
    Ok(unescape_legacy_entities(&captures[1]))
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::RustError;

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;
    use crate::text::{unescape, unescape_legacy_entities};

    const SUCCESS_CBOR_HEX: &str = "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974654e6f7465677061796c6f61646b4e6f74652026206d6f7265656572726f72f6";
    const ERROR_CBOR_HEX: &str = "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974654e6f7465677061796c6f6164f6656572726f72a2646b696e64656572726f72676d65737361676578214661766f72697465206e6f746520726573706f6e736520697320696e76616c6964";

    #[test]
    fn preserves_the_exact_textarea_and_entity_semantics() {
        let cases = [
            ("<textarea></textarea>", ""),
            (
                "prefix<textarea id='note' rows=4>  日本語\n&amp;&lt;&gt;&quot;&#039;&nbsp;&times;&unknown;&amp;lt;  </textarea>suffix",
                "  日本語\n&<>\"' ×&unknown;&lt;  ",
            ),
            (
                "<textarea>first</textarea><textarea>second</textarea>",
                "first",
            ),
            (
                "<textarea malformed>still accepted</textarea>",
                "still accepted",
            ),
            (
                "<textarea>&apos;&#39;&#x27;</textarea>",
                "&apos;&#39;&#x27;",
            ),
        ];
        for (html, expected) in cases {
            assert_eq!(parse(html).as_deref(), Ok(expected));
        }
    }

    #[test]
    fn skips_a_nonmatching_textarea_before_a_later_complete_match() {
        assert_eq!(
            parse("<textarea>has <b>markup</b></textarea><textarea>later</textarea>").as_deref(),
            Ok("later")
        );
    }

    #[test]
    fn rejects_missing_case_changed_and_incomplete_markup_without_leaking_input() {
        let secret = "sensitive-note-body";
        for html in [
            secret,
            "<TEXTAREA>upper</TEXTAREA>",
            "<textarea>missing close",
            "<textarea>has <b>markup</b></textarea>",
            "<textarea",
        ] {
            let error = parse(html).expect_err("nonmatching textarea grammar should fail");
            assert_eq!(error, RustError::Error(INVALID_RESPONSE.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn shared_text_helpers_preserve_distinct_legacy_contracts() {
        assert_eq!(
            unescape_legacy_entities("&amp;&lt;&gt;&quot;&#039;&nbsp;&times;&unknown;&amp;lt;"),
            "&<>\"' ×&unknown;&lt;"
        );
        assert_eq!(
            unescape("&amp;&apos;&#x41;&nbsp;&times;").expect("general XML entities should decode"),
            "&'A ×"
        );
        assert!(unescape("&unknown;").is_err());
    }

    #[test]
    fn versioned_success_and_error_envelopes_match_android_goldens() {
        let success = parse_envelope("<textarea>Note &amp; more</textarea>");
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(success.payload.as_deref(), Some("Note & more"));
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&success).expect("success envelope should encode")),
            SUCCESS_CBOR_HEX
        );

        let failure = parse_envelope("not a favorite-note response");
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(INVALID_RESPONSE.to_owned()))
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&failure).expect("error envelope should encode")),
            ERROR_CBOR_HEX
        );
    }
}
