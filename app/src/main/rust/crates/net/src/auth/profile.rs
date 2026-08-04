//! Authenticated account-profile response parsing.

use eh_domain::contracts::{AccountProfile, ApiEnvelope, RustError};
use tl::ParserOptions;

use super::{API_VERSION, MODULE};
use crate::html::{first_by_class, first_descendant_attr, handle_attr};
use crate::text::unescape;

const FORUMS_ORIGIN: &str = "https://forums.e-hentai.org";

/// Extracts the authenticated user's profile URL from the forum landing page.
///
/// # Errors
///
/// Returns [`RustError::NeedLogin`] when the authenticated user-links section or
/// its first link is absent. Malformed HTML syntax returns [`RustError::Error`].
pub fn parse_url(html: &str) -> Result<String, RustError> {
    let dom = parse_html(html, "profile URL")?;
    let parser = dom.parser();
    dom.get_element_by_id("userlinks")
        .and_then(|handle| handle.get(parser))
        .and_then(|node| node.as_tag())
        .and_then(|tag| tag.query_selector(parser, "a"))
        .and_then(|mut links| handle_attr(links.next()?, parser, "href"))
        .map(str::to_owned)
        .ok_or(RustError::NeedLogin)
}

/// Parses the forum profile page into Android presentation data.
///
/// Relative avatar paths are resolved against the forum origin. Missing avatars
/// are valid; a missing profile row or display-name node is a malformed response.
///
/// # Errors
///
/// Returns [`RustError::Error`] when HTML parsing fails or required profile fields
/// are absent.
pub fn parse(html: &str) -> Result<AccountProfile, RustError> {
    let dom = parse_html(html, "profile")?;
    let parser = dom.parser();
    let row = first_by_class(&dom, "row1")
        .ok_or_else(|| RustError::Error("profile row is missing".to_owned()))?;
    let Some(mut sections) = row
        .as_tag()
        .and_then(|tag| tag.query_selector(parser, "div"))
    else {
        return Err(RustError::Error("profile fields are missing".to_owned()));
    };
    let display_node = sections
        .next()
        .and_then(|handle| handle.get(parser))
        .ok_or_else(|| RustError::Error("profile display name is missing".to_owned()))?;
    let raw_name = display_node.inner_text(parser);
    let display_name = unescape(&raw_name)
        .as_deref()
        .unwrap_or(&raw_name)
        .trim()
        .to_owned();
    if display_name.is_empty() {
        return Err(RustError::Error("profile display name is empty".to_owned()));
    }

    let avatar = sections
        .next()
        .and_then(|handle| handle.get(parser))
        .and_then(|node| first_descendant_attr(node, parser, "src"))
        .map(normalize_avatar);
    Ok(AccountProfile {
        display_name,
        avatar,
    })
}

/// Parses a profile URL response into the versioned authentication envelope.
#[must_use]
pub fn parse_url_envelope(html: &str) -> ApiEnvelope<String> {
    match parse_url(html) {
        Ok(url) => ApiEnvelope::ok(API_VERSION, MODULE, url),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses a profile response into the versioned authentication envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<AccountProfile> {
    match parse(html) {
        Ok(profile) => ApiEnvelope::ok(API_VERSION, MODULE, profile),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

fn parse_html<'a>(html: &'a str, context: &str) -> Result<tl::VDom<'a>, RustError> {
    tl::parse(html, ParserOptions::default())
        .map_err(|error| RustError::Error(format!("invalid {context} HTML: {error}")))
}

fn normalize_avatar(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("{FORUMS_ORIGIN}/{}", url.trim_start_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::RustError;

    use super::{parse, parse_envelope, parse_url, parse_url_envelope};
    use crate::auth::{API_VERSION, MODULE};
    use crate::test_support::to_hex;

    const PROFILE_URL: &str = include_str!("../../../../test_data/profile_url.html");
    const PROFILE: &str = include_str!("../../test_data/auth/profile.html");
    const PROFILE_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656461757468677061796c6f6164a26b646973706c6179",
        "4e616d656f52656164657220262041727469737466617661746172782b68747470733a2f2f666f72",
        "756d732e652d68656e7461692e6f72672f656867742f6176617461722e706e67656572726f72f6",
    );

    #[test]
    fn parses_profile_url_and_reports_missing_login() {
        assert_eq!(
            parse_url(PROFILE_URL).expect("profile URL fixture should parse"),
            "https://forums.e-hentai.org/index.php?showuser=6"
        );
        assert_eq!(
            parse_url("<!doctype html><html><body></body></html>"),
            Err(RustError::NeedLogin)
        );
    }

    #[test]
    fn parses_display_name_and_relative_or_absolute_avatar() {
        let relative = parse(PROFILE).expect("profile fixture should parse");
        assert_eq!(relative.display_name, "Reader & Artist");
        assert_eq!(
            relative.avatar.as_deref(),
            Some("https://forums.e-hentai.org/ehgt/avatar.png")
        );

        let absolute = PROFILE.replace("/ehgt/avatar.png", "https://cdn.example/avatar.png");
        assert_eq!(
            parse(&absolute)
                .expect("absolute avatar should parse")
                .avatar
                .as_deref(),
            Some("https://cdn.example/avatar.png")
        );

        let bare_relative = PROFILE.replace("/ehgt/avatar.png", "avatar.png");
        assert_eq!(
            parse(&bare_relative)
                .expect("bare relative avatar should parse")
                .avatar
                .as_deref(),
            Some("https://forums.e-hentai.org/avatar.png")
        );
    }

    #[test]
    fn accepts_missing_avatar_and_rejects_missing_or_empty_name() {
        let without_avatar = PROFILE.replace("<img src=\"/ehgt/avatar.png\">", "");
        assert_eq!(
            parse(&without_avatar).expect("avatar is optional").avatar,
            None
        );
        assert_eq!(
            parse("<!doctype html><html><body></body></html>"),
            Err(RustError::Error("profile row is missing".to_owned()))
        );
        assert_eq!(
            parse("<!doctype html><html><body><section class='row1'></section></body></html>"),
            Err(RustError::Error(
                "profile display name is missing".to_owned()
            ))
        );
        let empty = PROFILE.replace("Reader &amp; Artist", "   ");
        assert_eq!(
            parse(&empty),
            Err(RustError::Error("profile display name is empty".to_owned()))
        );
    }

    #[test]
    fn wraps_profile_url_profile_and_error_in_auth_envelopes() {
        let url = parse_url_envelope(PROFILE_URL);
        assert_eq!(url.version, API_VERSION);
        assert_eq!(url.module, MODULE);
        assert!(url.is_ok());

        let profile = parse_envelope(PROFILE);
        assert_eq!(profile.version, API_VERSION);
        assert_eq!(profile.module, MODULE);
        assert_eq!(
            profile
                .payload
                .as_ref()
                .map(|value| value.display_name.as_str()),
            Some("Reader & Artist")
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&profile).expect("profile envelope should serialize")),
            PROFILE_GOLDEN_CBOR_HEX
        );

        let failure = parse_url_envelope("<html></html>");
        assert_eq!(failure.error, Some(RustError::NeedLogin));
    }
}
