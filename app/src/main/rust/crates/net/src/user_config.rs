//! Pure parsing for favorite-category names on the remote user-config page.
//!
//! This module deliberately preserves the legacy parser's narrow input grammar:
//! only the exact double-quoted `type`, `name`, and `value` attribute sequence is
//! recognized. Each failed entity decode is dropped independently, and an empty
//! or partial list is still a successful parser payload. Android retains HTTP,
//! redirects, cookies, credentials, preference mutation, and the final ten-item
//! settings invariant.

use eh_domain::contracts::{ApiEnvelope, FavoriteCategories};

use crate::text::unescape;

/// Current user-config parser protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "userConfig";

/// Parses favorite-category names into a V1 response envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<FavoriteCategories> {
    ApiEnvelope::ok(API_VERSION, MODULE, parse(input))
}

/// Extracts successfully decoded favorite-category names in source order.
///
/// Missing matches, malformed entities, duplicate indexes, and fewer than ten
/// decoded values preserve the legacy partial-success behavior. Android's
/// fixed-size preference setter decides whether the payload can be applied.
#[must_use]
pub fn parse(input: &str) -> FavoriteCategories {
    let names = regex!(r#"<input type="text" name="favorite_\d" value="([^"]+)""#)
        .captures_iter(input)
        .filter_map(|captures| {
            unescape(&captures[1])
                .ok()
                .map(std::borrow::Cow::into_owned)
        })
        .collect();
    FavoriteCategories { names }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::FavoriteCategories;

    use super::{API_VERSION, MODULE, parse, parse_envelope};

    #[test]
    fn parses_ten_names_and_supported_entities_from_offline_html() {
        let categories = parse(include_str!("../test_data/user_config.html"));
        assert_eq!(
            categories.names,
            [
                "Favorites 0",
                "Sci & Tech",
                "Space Name",
                "2 × 3",
                "日本語",
                "Category 5",
                "Category 6",
                "Category 7",
                "Category 8",
                "★ Category 9",
            ]
        );
    }

    #[test]
    fn keeps_strict_legacy_shape_source_order_duplicates_and_whitespace() {
        let input = concat!(
            r#"<input type="text" name="favorite_2" value="  first  ">"#,
            r#"<input type="text" name="favorite_2" value="second">"#,
            r#"<input name="favorite_3" type="text" value="wrong order">"#,
            r#"<input type='text' name='favorite_4' value='single quotes'>"#,
            r#"<input type="text" name="favorite_10" value="two digits">"#,
            r#"<input type="text" name="favorite_٥" value="non-ASCII digit">"#,
            r#"<input type="text" name="favorite_5" class="x" value="extra attribute">"#,
            r#"<input type="text" name="favorite_6" value="">"#,
        );
        assert_eq!(parse(input).names, ["  first  ", "second"]);
    }

    #[test]
    fn drops_only_invalid_entity_values_and_accepts_empty_or_partial_results() {
        let input = concat!(
            r#"<input type="text" name="favorite_0" value="before">"#,
            r#"<input type="text" name="favorite_1" value="bad &unknown; entity">"#,
            r#"<input type="text" name="favorite_2" value="after &amp; valid">"#,
        );
        assert_eq!(parse(input).names, ["before", "after & valid"]);
        assert_eq!(parse("").names, Vec::<String>::new());
        assert_eq!(parse("not html").names, Vec::<String>::new());
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(concat!(
            r#"<input type="text" name="favorite_0" value="One &amp; More">"#,
            r#"<input type="text" name="favorite_1" value="Two">"#,
        ));
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.payload,
            Some(FavoriteCategories {
                names: vec!["One & More".to_owned(), "Two".to_owned()],
            })
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            "a46776657273696f6e01666d6f64756c656a75736572436f6e666967677061796c6f6164a1656e616d6573826a4f6e652026204d6f72656354776f656572726f72f6"
        );
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        hex
    }
}
