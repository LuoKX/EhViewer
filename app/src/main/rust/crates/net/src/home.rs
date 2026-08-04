//! Home-page image-limit response parsing.

use eh_domain::contracts::{ApiEnvelope, ImageLimits, RustError};
use tl::{Node, ParserOptions};

use crate::html::first_by_class;

/// Current image-limits bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "imageLimits";

/// Parses home-page image limits into a V1 response envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<ImageLimits> {
    match parse(html) {
        Ok(limits) => ApiEnvelope::ok(API_VERSION, MODULE, limits),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses image-request quota values from the account home page.
///
/// Three numeric `strong` values represent current usage, maximum quota, and
/// reset cost. A single numeric value is the legacy sentinel shape: exact
/// `No restrictions` text maps to maximum zero, while every other shape maps to
/// maximum `-1`. Grouping commas are ignored and non-numeric values are skipped.
///
/// # Errors
///
/// Returns [`RustError::Error`] when the response is not HTML, the image-limit
/// panel is absent, or the remaining numeric-value count is neither one nor three.
pub fn parse(html: &str) -> Result<ImageLimits, RustError> {
    let dom = match tl::parse(html, ParserOptions::default()) {
        Ok(dom) => dom,
        Err(error) => {
            return Err(RustError::Error(format!(
                "invalid image-limits HTML: {error}"
            )));
        }
    };
    if dom.version().is_none() {
        return Err(RustError::Error(
            "image-limits response is not HTML".to_owned(),
        ));
    }
    let parser = dom.parser();
    let panel = first_by_class(&dom, "homebox")
        .and_then(Node::as_tag)
        .ok_or(RustError::Error("image-limits panel is missing".to_owned()))?;
    let values = panel
        .query_selector(parser, "strong")
        .ok_or(RustError::Error(
            "image-limits value query failed".to_owned(),
        ))?
        .filter_map(|handle| {
            handle
                .get(parser)?
                .inner_text(parser)
                .replace(',', "")
                .parse::<i32>()
                .ok()
        })
        .collect::<Vec<_>>();

    match values.as_slice() {
        [current, maximum, reset_cost] => Ok(ImageLimits {
            current: *current,
            maximum: *maximum,
            reset_cost: *reset_cost,
        }),
        [_] => Ok(ImageLimits {
            current: 0,
            maximum: if panel.inner_text(parser).contains("No restrictions") {
                0
            } else {
                -1
            },
            reset_cost: 0,
        }),
        _ => Err(RustError::Error(
            "image-limits value count is invalid".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{ApiEnvelope, ImageLimits, RustError};

    use super::{API_VERSION, MODULE, parse, parse_envelope};
    use crate::test_support::to_hex;

    const LIMITED_PAGE: &str = include_str!("../test_data/image_limits.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656b696d6167654c696d697473677061796c6f6164a367",
        "63757272656e741904d2676d6178696d756d191388697265736574436f7374193039656572726f72",
        "f6",
    );

    #[test]
    fn parses_grouped_limited_values_from_offline_html() {
        assert_eq!(
            parse(LIMITED_PAGE),
            Ok(ImageLimits {
                current: 1_234,
                maximum: 5_000,
                reset_cost: 12_345,
            })
        );
    }

    #[test]
    fn preserves_unrestricted_and_restricted_single_value_sentinels() {
        let unrestricted = html("No restrictions on image usage", "<strong>0</strong>");
        assert_eq!(
            parse(&unrestricted),
            Ok(ImageLimits {
                current: 0,
                maximum: 0,
                reset_cost: 0,
            })
        );

        let restricted = html("Image usage is restricted", "<strong>999</strong>");
        assert_eq!(
            parse(&restricted),
            Ok(ImageLimits {
                current: 0,
                maximum: -1,
                reset_cost: 0,
            })
        );
    }

    #[test]
    fn keeps_legacy_numeric_filtering_and_source_order() {
        let page = html(
            "Limits",
            concat!(
                "<strong>invalid</strong>",
                "<strong>-2</strong>",
                "<strong>3,000</strong>",
                "<strong>4</strong>",
            ),
        );
        assert_eq!(
            parse(&page),
            Ok(ImageLimits {
                current: -2,
                maximum: 3_000,
                reset_cost: 4,
            })
        );

        let overflow = html(
            "No restrictions",
            "<strong>999999999999999999999</strong><strong>1</strong>",
        );
        assert_eq!(
            parse(&overflow),
            Ok(ImageLimits {
                current: 0,
                maximum: 0,
                reset_cost: 0,
            })
        );
    }

    #[test]
    fn rejects_missing_non_html_and_invalid_value_counts() {
        let cases = [
            ("blocked".to_owned(), "not HTML"),
            (
                "<!doctype html><html><body></body></html>".to_owned(),
                "panel is missing",
            ),
            (html("Limits", ""), "count is invalid"),
            (
                html("Limits", "<strong>1</strong><strong>2</strong>"),
                "count is invalid",
            ),
            (
                html(
                    "Limits",
                    concat!(
                        "<strong>1</strong><strong>2</strong>",
                        "<strong>3</strong><strong>4</strong>",
                    ),
                ),
                "count is invalid",
            ),
        ];
        for (input, expected) in cases {
            let error = parse(&input).expect_err("invalid limit markup must be rejected");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn wraps_success_and_error_in_the_versioned_envelope() {
        let success = parse_envelope(LIMITED_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(
            success.payload.as_ref().map(|limits| limits.maximum),
            Some(5_000)
        );
        assert!(success.is_ok());

        let failure = parse_envelope("<!doctype html><html><body></body></html>");
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert!(matches!(failure.error, Some(RustError::Error(_))));
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            ImageLimits {
                current: 1_234,
                maximum: 5_000,
                reset_cost: 12_345,
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<ImageLimits> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    fn html(text: &str, values: &str) -> String {
        format!(
            "<!doctype html><html><body><div class='homebox'>{text}{values}</div></body></html>"
        )
    }
}
