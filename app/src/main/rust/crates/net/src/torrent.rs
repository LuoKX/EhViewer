//! Pure parsing for gallery torrent-list responses.
//!
//! The table grammar and per-row skip behavior intentionally follow the legacy
//! parser. The cross-layer contract is narrower and safer: raw `onclick` URLs
//! never leave Rust. A row is returned only when its final path segment is exactly
//! a 40-hex info hash followed by `.torrent`; the hash is canonicalized to lowercase.
//! Android retains HTTP, gallery/tracker tokens, magnet construction, intents,
//! clipboard fallback, and UI.

use eh_domain::contracts::{ApiEnvelope, RustError, TorrentEntry, TorrentListPage};
use tl::ParserOptions;

use crate::text::unescape;

/// Current torrent-list parser protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "torrentList";

/// Parses one gallery torrent response into a V1 envelope.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<TorrentListPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses torrent tables while omitting expunged, malformed, or unsafe rows.
///
/// Empty and non-HTML text preserve the legacy empty-list success behavior.
///
/// # Errors
///
/// Returns [`RustError::Error`] only when the HTML parser or source-controlled
/// table selector cannot be constructed. Error text never includes response data.
pub fn parse(html: &str) -> Result<TorrentListPage, RustError> {
    let dom = match tl::parse(html, ParserOptions::default()) {
        Ok(dom) => dom,
        Err(error) => {
            return Err(RustError::Error(format!(
                "invalid torrent-list HTML: {error}"
            )));
        }
    };
    let parser = dom.parser();
    let tables = dom.query_selector("table").ok_or(RustError::Error(
        "torrent-list table query failed".to_owned(),
    ))?;
    let torrents = tables
        .filter_map(|handle| handle.get(parser))
        .filter_map(|table| parse_table(&table.inner_html(parser)))
        .collect();
    Ok(TorrentListPage { torrents })
}

fn parse_table(html: &str) -> Option<TorrentEntry> {
    if html.contains("Expunged") {
        return None;
    }
    let groups = regex!(
        "<span( style=\"color:red\")?>([0-9-]+) [0-9:]+</span>[\\s\\S]+</span> ([0-9.]+ [KMGT]iB)</td>[\\s\\S]+</span> ([0-9]+)</td>[\\s\\S]+</span> ([0-9]+)</td>[\\s\\S]+</span> ([0-9]+)</td>[\\s\\S]+</span> ([^<]+)</td>[\\s\\S]+onclick=\"document.location='([^\"]+)'[^<]+>([^<]+)</a>"
    )
    .captures(html)?;
    Some(TorrentEntry {
        outdated: groups.get(1).is_some(),
        posted: groups[2].to_owned(),
        size: groups[3].to_owned(),
        seeds: groups[4].parse().ok()?,
        peers: groups[5].parse().ok()?,
        downloads: groups[6].parse().ok()?,
        uploader: groups[7].to_owned(),
        info_hash: extract_info_hash(&groups[8])?,
        name: unescape(&groups[9]).ok()?.into_owned(),
    })
}

fn extract_info_hash(raw_url: &str) -> Option<String> {
    let file_name = raw_url.rsplit('/').next()?;
    let hash = file_name.strip_suffix(".torrent")?;
    (hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{TorrentEntry, TorrentListPage};

    use super::{API_VERSION, MODULE, extract_info_hash, parse, parse_envelope};
    use crate::test_support::to_hex;

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";
    const UPPER_HASH: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    #[test]
    fn parses_normal_and_outdated_rows_from_offline_html() {
        let page = parse(include_str!("../test_data/torrent_list.html"))
            .expect("offline torrent fixture should parse");
        assert_eq!(
            page,
            TorrentListPage {
                torrents: vec![
                    TorrentEntry {
                        outdated: false,
                        posted: "2026-08-03".to_owned(),
                        size: "1.5 MiB".to_owned(),
                        seeds: 12,
                        peers: 3,
                        downloads: 45,
                        uploader: "Alice".to_owned(),
                        info_hash: HASH.to_owned(),
                        name: "First & More".to_owned(),
                    },
                    TorrentEntry {
                        outdated: true,
                        posted: "2025-01-02".to_owned(),
                        size: "2 GiB".to_owned(),
                        seeds: 0,
                        peers: 1,
                        downloads: 2,
                        uploader: "Bob".to_owned(),
                        info_hash: UPPER_HASH.to_ascii_lowercase(),
                        name: "Old × Copy".to_owned(),
                    },
                ],
            }
        );
    }

    #[test]
    fn skips_expunged_malformed_unsafe_and_individually_invalid_rows() {
        let input = [
            row("", "1", "2", "3", HASH, "kept"),
            row("", "1", "2", "3", "short.torrent", "bad hash"),
            row("", "1", "2", "3", HASH, "bad &unknown; name"),
            row("", "2147483648", "2", "3", HASH, "overflow"),
            "<table>Expunged</table>".to_owned(),
            "<table>not a torrent row</table>".to_owned(),
        ]
        .join("");
        let page = parse(&input).expect("mixed rows should parse");
        assert_eq!(page.torrents.len(), 1);
        assert_eq!(page.torrents[0].name, "kept");
    }

    #[test]
    fn validates_only_the_final_hash_filename_without_assuming_a_host() {
        assert_eq!(
            extract_info_hash(&format!("{HASH}.torrent")),
            Some(HASH.to_owned())
        );
        assert_eq!(
            extract_info_hash(&format!("/relative/path/{UPPER_HASH}.torrent")),
            Some(UPPER_HASH.to_ascii_lowercase())
        );
        assert_eq!(
            extract_info_hash(&format!("https://dynamic.example/get/{HASH}.torrent")),
            Some(HASH.to_owned())
        );
        for invalid in [
            "short.torrent",
            "gggggggggggggggggggggggggggggggggggggggg.torrent",
            "0123456789abcdef0123456789abcdef01234567.TORRENT",
            "0123456789abcdef0123456789abcdef01234567.torrent?key=1",
            "0123456789abcdef0123456789abcdef01234567.torrent#fragment",
            "0123456789abcdef0123456789abcdef012345678.torrent",
        ] {
            assert_eq!(extract_info_hash(invalid), None, "accepted {invalid}");
        }
    }

    #[test]
    fn empty_and_non_html_inputs_are_successful_empty_pages() {
        for input in ["", "not html", "<div>no tables</div>"] {
            assert_eq!(
                parse(input).expect("legacy empty shape should succeed"),
                TorrentListPage {
                    torrents: Vec::new()
                }
            );
        }
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(&row(" style=\"color:red\"", "7", "8", "9", HASH, "Name"));
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.payload.as_ref().map(|page| page.torrents.len()),
            Some(1)
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            "a46776657273696f6e01666d6f64756c656b746f7272656e744c697374677061796c6f6164a168746f7272656e747381a9686f75746461746564f566706f737465646a323032362d30382d30336473697a6567312e35204d6942657365656473076570656572730869646f776e6c6f616473096875706c6f616465726875706c6f6164657268696e666f48617368782830313233343536373839616263646566303132333435363738396162636465663031323334353637646e616d65644e616d65656572726f72f6"
        );
    }

    fn row(
        style: &str,
        seeds: &str,
        peers: &str,
        downloads: &str,
        hash_file_name: &str,
        name: &str,
    ) -> String {
        format!(
            "<table><tr><td><span{style}>2026-08-03 12:34</span></td><td><span>x</span> 1.5 MiB</td><td><span>x</span> {seeds}</td><td><span>x</span> {peers}</td><td><span>x</span> {downloads}</td><td><span>x</span> uploader</td><td><a onclick=\"document.location='/get/{hash_file_name}.torrent'; return false\">{name}</a></td></tr></table>"
        )
    }
}
