//! Gallery archive-list and final-download response parsing.

use eh_domain::contracts::{
    ApiEnvelope, ArchiveDownloadTarget, ArchiveFunds, ArchiveListPage, ArchiveOption, RustError,
};
use tl::{HTMLTag, Node, Parser, ParserOptions, VDom};
use url::Url;

use crate::html::node_attr;

const ERROR_NEED_HATH_CLIENT: &str =
    "You must have a H@H client assigned to your account to use this feature.";
const ERROR_INSUFFICIENT_FUNDS: &str = "You do not have enough funds to download this archive.";

/// Current gallery-archive bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryArchive";

/// Parses archive choices and optional balances into a V1 response envelope.
#[must_use]
pub fn parse_list_envelope(html: &str, include_funds: bool) -> ApiEnvelope<ArchiveListPage> {
    match parse_list(html, include_funds) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses a validated final archive target into a V1 response envelope.
#[must_use]
pub fn parse_target_envelope(html: &str) -> ApiEnvelope<ArchiveDownloadTarget> {
    match parse_target(html) {
        Ok(target) => ApiEnvelope::ok(API_VERSION, MODULE, target),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses available direct and H@H archive choices.
///
/// `include_funds` must be `false` when Android already fetched the active
/// account's balances from a separate endpoint. The returned option order and
/// legacy display strings are preserved for the existing archive dialog.
///
/// # Errors
///
/// Returns [`RustError::Error`] when required archive markup or requested balance
/// values are missing or malformed.
pub fn parse_list(html: &str, include_funds: bool) -> Result<ArchiveListPage, RustError> {
    let dom = parse_html(html)?;
    let parser = dom.parser();
    let mut options = parse_direct_options(&dom, parser)?;
    options.extend(parse_hath_options(html));
    let funds = include_funds.then(|| parse_funds(html)).transpose()?;
    Ok(ArchiveListPage { options, funds })
}

/// Parses and validates the final archive download URL.
///
/// A response without the `continue` element is a successful pending result so
/// Android may preserve its existing one-shot retry. A present link must be an
/// absolute HTTPS URL without credentials or a fragment. Existing query fields
/// are preserved, and exactly one `start=1` field is required in the result.
///
/// # Errors
///
/// Returns typed H@H/funds failures for the two established remote messages and
/// [`RustError::Error`] for malformed or unsafe final targets.
pub fn parse_target(html: &str) -> Result<ArchiveDownloadTarget, RustError> {
    if html.contains(ERROR_NEED_HATH_CLIENT) {
        return Err(RustError::NoHathClient);
    }
    if html.contains(ERROR_INSUFFICIENT_FUNDS) {
        return Err(RustError::InsufficientFunds);
    }

    let dom = parse_html(html)?;
    let parser = dom.parser();
    let Some(continue_handle) = dom.get_element_by_id("continue") else {
        return Ok(ArchiveDownloadTarget { url: None });
    };
    let continue_node = continue_handle.get(parser).ok_or(RustError::Error(
        "gallery archive continue element is missing".to_owned(),
    ))?;
    let continue_tag = continue_node.as_tag().ok_or(RustError::Error(
        "gallery archive continue element is malformed".to_owned(),
    ))?;
    let anchor = first_descendant(continue_tag, parser, "a")
        .ok_or_else(|| RustError::Error("gallery archive continue link is missing".to_owned()))?;
    let href = anchor
        .attributes()
        .get("href")
        .flatten()
        .and_then(|value| value.try_as_utf8_str())
        .ok_or_else(|| RustError::Error("gallery archive target is missing".to_owned()))?;
    let href = href.replace("&amp;", "&");
    let url = validate_target(&href)?;
    Ok(ArchiveDownloadTarget { url: Some(url) })
}

fn parse_html(html: &str) -> Result<VDom<'_>, RustError> {
    let dom = match tl::parse(html, ParserOptions::default().track_ids()) {
        Ok(dom) => dom,
        Err(error) => {
            return Err(RustError::Error(format!(
                "invalid gallery-archive HTML: {error}"
            )));
        }
    };
    if dom.version().is_none() {
        return Err(RustError::Error(
            "gallery archive response is not HTML".to_owned(),
        ));
    }
    Ok(dom)
}

fn parse_direct_options(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<Vec<ArchiveOption>, RustError> {
    let container = dom
        .get_element_by_id("db")
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
        .and_then(|database| first_descendant(database, parser, "div"))
        .ok_or_else(|| RustError::Error("gallery archive choices are missing".to_owned()))?;
    let children = container.children();
    Ok(children
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser))
        .filter_map(|node| parse_direct_option(node, parser))
        .collect())
}

fn parse_direct_option(node: &Node<'_>, parser: &Parser<'_>) -> Option<ArchiveOption> {
    let tag = node.as_tag()?;
    let style = node_attr(node, "style")?;
    if tag.name() != "div" || style.contains("color:#CCCCCC") {
        return None;
    }

    let form = first_descendant(tag, parser, "form")?;
    let input = first_descendant(form, parser, "input")?;
    let size = first_descendant(first_descendant(tag, parser, "p")?, parser, "strong")?;
    let cost = first_descendant(first_descendant(tag, parser, "div")?, parser, "strong")?;
    Some(ArchiveOption {
        resolution: input
            .attributes()
            .get("value")
            .flatten()?
            .try_as_utf8_str()?
            .to_owned(),
        name: String::new(),
        size: size.inner_text(parser).into_owned(),
        cost: cost.inner_text(parser).replace(',', ""),
        is_hath: false,
    })
}

fn parse_hath_options(html: &str) -> Vec<ArchiveOption> {
    regex!(
        r#"<p><a href="[^"]*" onclick="return do_hathdl\('([0-9]+|org)'\)">([^<]+)</a></p>\s*<p>([\w. ]+)</p>\s*<p>([\w. ]+)</p>"#
    )
    .captures_iter(html)
    .map(|capture| ArchiveOption {
        resolution: capture[1].to_owned(),
        name: capture[2].to_owned(),
        size: capture[3].to_owned(),
        cost: capture[4].to_owned(),
        is_hath: true,
    })
    .collect()
}

fn parse_funds(html: &str) -> Result<ArchiveFunds, RustError> {
    let capture = regex!(r"<p>([\d,]+) GP \[[^]]*] &nbsp; ([\d,]+) Credits \[[^]]*]</p>")
        .captures(html)
        .ok_or_else(|| RustError::Error("gallery archive funds are missing".to_owned()))?;
    Ok(ArchiveFunds {
        gp: parse_number(&capture[1]) / 1_000,
        credit: parse_number(&capture[2]),
    })
}

fn parse_number(value: &str) -> i32 {
    value.replace(',', "").parse().unwrap_or(0)
}

fn validate_target(href: &str) -> Result<String, RustError> {
    if href.trim() != href || href.chars().any(char::is_control) {
        return Err(RustError::Error(
            "gallery archive target contains invalid characters".to_owned(),
        ));
    }
    let mut url = Url::parse(href)
        .map_err(|_| RustError::Error("gallery archive target is malformed".to_owned()))?;
    if url.scheme() != "https" {
        return Err(RustError::Error(
            "gallery archive target must use HTTPS".to_owned(),
        ));
    }
    if url.host_str().is_none() {
        return Err(RustError::Error(
            "gallery archive target host is missing".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(RustError::Error(
            "gallery archive target credentials are forbidden".to_owned(),
        ));
    }
    if url.fragment().is_some() {
        return Err(RustError::Error(
            "gallery archive target fragment is forbidden".to_owned(),
        ));
    }

    let start_values = url
        .query_pairs()
        .filter_map(|(name, value)| (name == "start").then_some(value.into_owned()))
        .collect::<Vec<_>>();
    match start_values.as_slice() {
        [] => {
            url.query_pairs_mut().append_pair("start", "1");
        }
        [value] if value == "1" => {}
        _ => {
            return Err(RustError::Error(
                "gallery archive target has a conflicting start query".to_owned(),
            ));
        }
    }
    Ok(url.into())
}

fn first_descendant<'a>(
    tag: &'a HTMLTag<'_>,
    parser: &'a Parser<'_>,
    selector: &str,
) -> Option<&'a HTMLTag<'a>> {
    tag.query_selector(parser, selector)?
        .next()?
        .get(parser)?
        .as_tag()
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{
        ApiEnvelope, ArchiveDownloadTarget, ArchiveFunds, ArchiveListPage, ArchiveOption, RustError,
    };

    use super::{
        API_VERSION, ERROR_INSUFFICIENT_FUNDS, ERROR_NEED_HATH_CLIENT, MODULE, parse_list,
        parse_list_envelope, parse_target, parse_target_envelope,
    };
    use crate::test_support::to_hex;

    const ARCHIVE_PAGE: &str = include_str!("../../../../test_data/archives_with_funds.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656e67616c6c65727941726368697665677061796c6f61",
        "64a26b617263686976654c69737481a563726573636f7267646e616d65606473697a656931352e34",
        "372047694264636f73746937343737303820475066697348617468f46566756e6473a26267701a00",
        "0b532d666372656469741a5d0b0ae4656572726f72f6",
    );
    const V1_TARGET_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656e67616c6c65727941726368697665677061796c6f61",
        "64a16375726c783068747470733a2f2f617263686976652e6578616d706c652f66696c653f746f6b",
        "656e3d61253246622673746172743d31656572726f72f6",
    );

    #[test]
    fn parses_direct_hath_and_fund_values_from_offline_html() {
        let page = parse_list(ARCHIVE_PAGE, true).expect("checked-in archive HTML should parse");
        assert_eq!(
            page,
            ArchiveListPage {
                options: vec![
                    ArchiveOption {
                        resolution: "org".to_owned(),
                        name: String::new(),
                        size: "15.47 GiB".to_owned(),
                        cost: "747708 GP".to_owned(),
                        is_hath: false,
                    },
                    ArchiveOption {
                        resolution: "org".to_owned(),
                        name: "Original".to_owned(),
                        size: "15.47 GiB".to_owned(),
                        cost: "332315 GP".to_owned(),
                        is_hath: true,
                    },
                ],
                funds: Some(ArchiveFunds {
                    gp: 742_189,
                    credit: 1_561_004_772,
                }),
            }
        );
    }

    #[test]
    fn omits_funds_when_android_supplies_them_separately() {
        let page = parse_list(ARCHIVE_PAGE, false).expect("archive choices should parse");
        assert_eq!(page.funds, None);
        assert_eq!(page.options.len(), 2);
    }

    #[test]
    fn preserves_legacy_skips_and_empty_hath_baseline() {
        let html = concat!(
            "<!doctype html><html><body><div id='db'><div>",
            "<div style='color:#CCCCCC'><form><input value='res'></form></div>",
            "<div style='clear:both'></div>",
            "</div></div></body></html>",
        );
        let page = parse_list(html, false).expect("disabled choices should be ignored");
        assert!(page.options.is_empty());
        assert_eq!(page.funds, None);
    }

    #[test]
    fn rejects_missing_archive_markup_or_requested_funds() {
        let missing_choices = parse_list("<!doctype html><html><body></body></html>", false)
            .expect_err("archive choices are required");
        assert!(missing_choices.to_string().contains("choices are missing"));

        let missing_funds = parse_list(
            "<!doctype html><html><body><div id='db'><div></div></div></body></html>",
            true,
        )
        .expect_err("requested funds are required");
        assert!(missing_funds.to_string().contains("funds are missing"));

        let not_html = parse_list("blocked", false).expect_err("non-HTML must be rejected");
        assert!(not_html.to_string().contains("not HTML"));
    }

    #[test]
    fn parses_safe_targets_and_preserves_unrelated_query_fields() {
        let cases = [
            (
                target_html("https://archive.example/file"),
                "https://archive.example/file?start=1",
            ),
            (
                target_html("https://archive.example/file?token=a%2Fb"),
                "https://archive.example/file?token=a%2Fb&start=1",
            ),
            (
                target_html("https://archive.example/file?token=a%2Fb&amp;server=1"),
                "https://archive.example/file?token=a%2Fb&server=1&start=1",
            ),
            (
                target_html("https://archive.example/file?start=1"),
                "https://archive.example/file?start=1",
            ),
            (
                target_html("https://archive.example/file?st%61rt=%31"),
                "https://archive.example/file?st%61rt=%31",
            ),
        ];
        for (html, expected) in cases {
            assert_eq!(
                parse_target(&html)
                    .expect("safe target should parse")
                    .url
                    .as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn keeps_missing_continue_as_a_successful_pending_target() {
        assert_eq!(
            parse_target("<!doctype html><html><body><p>Preparing</p></body></html>"),
            Ok(ArchiveDownloadTarget { url: None })
        );
    }

    #[test]
    fn maps_established_remote_failures_before_target_extraction() {
        assert_eq!(
            parse_target(ERROR_NEED_HATH_CLIENT),
            Err(RustError::NoHathClient)
        );
        assert_eq!(
            parse_target(ERROR_INSUFFICIENT_FUNDS),
            Err(RustError::InsufficientFunds)
        );
    }

    #[test]
    fn rejects_unsafe_or_ambiguous_download_targets() {
        let cases = [
            ("http://archive.example/file", "must use HTTPS"),
            ("javascript:alert(1)", "must use HTTPS"),
            ("/relative/file", "malformed"),
            ("https://user:secret@archive.example/file", "credentials"),
            ("https://archive.example/file#fragment", "fragment"),
            ("https://archive.example/file?start=2", "conflicting start"),
            (
                "https://archive.example/file?start=1&start=1",
                "conflicting start",
            ),
            (" https://archive.example/file", "invalid characters"),
            ("https://archive.example/file\n", "invalid characters"),
        ];
        for (href, expected) in cases {
            let error = parse_target(&target_html(href))
                .expect_err("unsafe archive target must be rejected");
            assert!(error.to_string().contains(expected), "{href}: {error}");
        }
    }

    #[test]
    fn rejects_malformed_present_continue_elements() {
        let missing_link =
            parse_target("<!doctype html><html><body><p id='continue'>Preparing</p></body></html>")
                .expect_err("present continue element requires a link");
        assert!(missing_link.to_string().contains("link is missing"));

        let missing_href = parse_target(
            "<!doctype html><html><body><p id='continue'><a>Continue</a></p></body></html>",
        )
        .expect_err("present continue link requires a target");
        assert!(missing_href.to_string().contains("target is missing"));
    }

    #[test]
    fn wraps_list_target_and_failures_in_versioned_envelopes() {
        let list = parse_list_envelope(ARCHIVE_PAGE, true);
        assert_eq!(list.version, API_VERSION);
        assert_eq!(list.module, MODULE);
        assert_eq!(
            list.payload.as_ref().map(|page| page.options.len()),
            Some(2)
        );
        assert!(list.is_ok());

        let target = parse_target_envelope(&target_html("https://archive.example/file"));
        assert_eq!(target.version, API_VERSION);
        assert_eq!(target.module, MODULE);
        assert!(target.is_ok());

        let failure = parse_target_envelope(ERROR_INSUFFICIENT_FUNDS);
        assert_eq!(failure.error, Some(RustError::InsufficientFunds));
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            ArchiveListPage {
                options: vec![ArchiveOption {
                    resolution: "org".to_owned(),
                    name: String::new(),
                    size: "15.47 GiB".to_owned(),
                    cost: "747708 GP".to_owned(),
                    is_hath: false,
                }],
                funds: Some(ArchiveFunds {
                    gp: 742_189,
                    credit: 1_561_004_772,
                }),
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<ArchiveListPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn target_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            ArchiveDownloadTarget {
                url: Some("https://archive.example/file?token=a%2Fb&start=1".to_owned()),
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 target envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_TARGET_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<ArchiveDownloadTarget> =
            serde_cbor::from_slice(&encoded).expect("golden target envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    fn target_html(href: &str) -> String {
        format!(
            "<!doctype html><html><body><p id='continue'><a href=\"{href}\">Continue</a></p></body></html>"
        )
    }
}
