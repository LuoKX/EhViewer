//! Sign-in response parsing.

use eh_domain::contracts::{ApiEnvelope, RustError, SignInIdentity};

use super::{API_VERSION, MODULE};

const INVALID_RESPONSE: &str = "unrecognized sign-in response";

/// Parses the forum sign-in response into the authenticated identity.
///
/// # Errors
///
/// Returns [`RustError::Error`] with the server-provided message when sign-in is
/// rejected. An unrecognized response also returns [`RustError::Error`] and must
/// be presented through the Android error mapper rather than logged with its HTML.
pub fn parse(html: &str) -> Result<SignInIdentity, RustError> {
    if let Some(captures) = regex!(r"<p>You are now logged in as: (.+?)<").captures(html) {
        return Ok(SignInIdentity {
            username: captures[1].to_owned(),
        });
    }

    let error = regex!(
        r#"<h4>The error returned was:</h4>\s*<p>(.+?)</p>|<span class="postcolor">(.+?)</span>"#
    )
    .captures(html)
    .and_then(|captures| captures.get(1).or_else(|| captures.get(2)))
    .map(|capture| capture.as_str().to_owned());
    Err(RustError::Error(
        error.unwrap_or_else(|| INVALID_RESPONSE.to_owned()),
    ))
}

/// Parses sign-in HTML into the versioned authentication envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<SignInIdentity> {
    match parse(html) {
        Ok(identity) => ApiEnvelope::ok(API_VERSION, MODULE, identity),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::RustError;

    use super::{parse, parse_envelope};
    use crate::auth::{API_VERSION, MODULE};
    use crate::test_support::to_hex;

    const SUCCESS: &str = include_str!("../../test_data/auth/sign_in_success.html");
    const PRIMARY_ERROR: &str = include_str!("../../test_data/auth/sign_in_error.html");
    const SECONDARY_ERROR: &str = include_str!("../../test_data/auth/sign_in_error_legacy.html");
    const IDENTITY_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656461757468677061796c6f6164a168757365726e616d",
        "6566526561646572656572726f72f6",
    );

    #[test]
    fn parses_success_and_both_server_error_shapes() {
        assert_eq!(
            parse(SUCCESS)
                .expect("success fixture should parse")
                .username,
            "Reader"
        );
        assert_eq!(
            parse(PRIMARY_ERROR),
            Err(RustError::Error("Wrong password".to_owned()))
        );
        assert_eq!(
            parse(SECONDARY_ERROR),
            Err(RustError::Error("Account temporarily locked".to_owned()))
        );
    }

    #[test]
    fn rejects_unknown_response_without_copying_html_into_the_error() {
        let html = "<!doctype html><html><body>unexpected secret body</body></html>";
        let error = parse(html).expect_err("unknown response must fail");
        assert_eq!(
            error,
            RustError::Error("unrecognized sign-in response".to_owned())
        );
        assert!(!error.to_string().contains("secret body"));
    }

    #[test]
    fn wraps_success_and_failure_in_the_auth_envelope() {
        let success = parse_envelope(SUCCESS);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(
            success
                .payload
                .as_ref()
                .map(|identity| identity.username.as_str()),
            Some("Reader")
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&success).expect("identity envelope should serialize")),
            IDENTITY_GOLDEN_CBOR_HEX
        );

        let failure = parse_envelope(PRIMARY_ERROR);
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(
            failure.error,
            Some(RustError::Error("Wrong password".to_owned()))
        );
    }
}
