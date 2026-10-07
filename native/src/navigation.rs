//! Token-free presentation of the existing portable navigation parser.

use eh_net::gallery::navigation::{
    API_VERSION, GalleryNavigationKind, GalleryNavigationRequest, GalleryNavigationTarget, parse,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Grammar {
    #[default]
    Detail,
    Page,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Inspection {
    Detail { gid: i64 },
    Page { gid: i64, page_index: i32 },
    NoMatch,
    Rejected,
}

pub(crate) fn inspect(input: &str, grammar: Grammar) -> Inspection {
    let request = GalleryNavigationRequest {
        version: API_VERSION,
        target: match grammar {
            Grammar::Detail => GalleryNavigationTarget::Detail,
            Grammar::Page => GalleryNavigationTarget::Page,
        },
        input: input.to_owned(),
        strict: true,
    };
    // Call the canonical owner. No JNI/CBOR, copied regex, or second algorithm.
    let Ok(plan) = parse(&request) else {
        return Inspection::Rejected;
    };
    match (plan.kind, plan.gid, plan.page_index) {
        (GalleryNavigationKind::Detail, Some(gid), None) => Inspection::Detail { gid },
        (GalleryNavigationKind::Page, Some(gid), Some(page_index)) => {
            Inspection::Page { gid, page_index }
        }
        (GalleryNavigationKind::None, None, None) => Inspection::NoMatch,
        _ => Inspection::Rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_and_mpv_reuse_the_core_without_retaining_tokens() {
        for input in [
            "https://e-hentai.org/g/42/0123456789",
            "http://exhentai.org/mpv/42/0123456789",
        ] {
            let result = inspect(input, Grammar::Detail);
            assert_eq!(result, Inspection::Detail { gid: 42 });
            assert!(!format!("{result:?}").contains("0123456789"));
        }
    }

    #[test]
    fn page_index_and_legacy_zero_sentinel_are_preserved() {
        for (suffix, expected) in [("1", 0), ("12", 11), ("0", -1)] {
            let input = format!("https://e-hentai.org/s/0123456789/42-{suffix}");
            assert_eq!(
                inspect(&input, Grammar::Page),
                Inspection::Page {
                    gid: 42,
                    page_index: expected
                },
            );
        }
    }

    #[test]
    fn unsupported_hosts_wrong_grammar_empty_and_overflow_do_not_match() {
        for (input, grammar) in [
            ("", Grammar::Detail),
            ("https://example.com/g/42/0123456789", Grammar::Detail),
            ("https://e-hentai.org/g/42/0123456789", Grammar::Page),
            (
                "https://e-hentai.org/g/9223372036854775808/0123456789",
                Grammar::Detail,
            ),
        ] {
            assert_eq!(inspect(input, grammar), Inspection::NoMatch);
        }
    }

    #[test]
    fn inspector_does_not_reinterpret_the_legacy_substring_contract() {
        assert_eq!(
            inspect(
                "prefix https://e-hentai.org/g/42/0123456789 suffix",
                Grammar::Detail
            ),
            Inspection::Detail { gid: 42 },
        );
    }

    #[test]
    fn oversized_input_is_rejected_with_no_input_in_the_result() {
        let input = format!("private-input-{}", "x".repeat(16 * 1024));
        let result = inspect(&input, Grammar::Detail);
        assert_eq!(result, Inspection::Rejected);
        assert_eq!(format!("{result:?}"), "Rejected");
    }
}
