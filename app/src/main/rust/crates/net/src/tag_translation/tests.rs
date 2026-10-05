#![allow(
    clippy::float_cmp,
    reason = "exact Float compatibility is part of the wire contract"
)]

use super::*;

const CACHED: &str = r#"{"n":{"female":"女性","male":"男性"},"f":{"zz":"同","aa":"同","big aa":"别名","other":"aa 提示","empty":"  "},"m":{"aa":"男性"},"a":{"artist":"作者"}}"#;
fn index() -> TagIndex {
    TagIndex::parse(CACHED.as_bytes(), false).unwrap()
}
fn query(keyword: &str, translate: bool, limit: usize) -> SuggestionRequest {
    SuggestionRequest {
        version: 1,
        keyword: keyword.to_owned(),
        translate,
        limit,
    }
}
#[test]
fn parses_remote_and_cached_dictionaries_and_looks_up_trimmed_names() {
    let raw = r#"{"ignored":true,"data":[{"namespace":"rows","data":{"female":{"name":" 女 ","intro":"ignored"}}},{"namespace":"female","data":{"aa":{"name":"  提示  "}}}]}"#;
    let raw = TagIndex::parse(raw.as_bytes(), true).unwrap();
    assert_eq!(raw.translation("n", "female"), Some("女"));
    assert_eq!(raw.translation("f", "aa"), Some("提示"));
    assert_eq!(index().translation("f", "empty"), None);
    assert_eq!(index().translation("absent", "aa"), None);
    assert_eq!(index().translation("f", "absent"), None);
    let whitespace = TagIndex::parse(
        "{\"f\":{\"x\":\"\\u001c x \\u00a0\",\"y\":\"\\u0085\"}}".as_bytes(),
        false,
    )
    .unwrap();
    assert_eq!(whitespace.translation("f", "x"), Some("x"));
    assert_eq!(whitespace.translation("f", "y"), Some("\u{0085}"));
}
#[test]
fn preserves_prefix_order_specific_order_word_and_translated_matches() {
    let index = index();
    let rows = index.suggest(&query("-~tag:weak:f:aa", true, 50)).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.tag.as_str()).collect::<Vec<_>>(),
        [
            "-~tag:weak:f:aa",
            "-~tag:weak:f:other",
            "-~tag:weak:f:big aa"
        ]
    );
    assert_eq!(
        rows.iter().map(|row| row.score).collect::<Vec<_>>(),
        [2.0, 5.0, 12.0]
    );
    assert_eq!(rows[0].hint.as_deref(), Some("同"));
    let rows = index.suggest(&query("female:aa", false, 50)).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.hint.is_none()));
    assert_eq!(index.suggest(&query("tag:weak:", true, 50)).unwrap(), []);
    assert_eq!(index.suggest(&query("aa", false, 0)).unwrap(), []);
}
#[test]
fn weights_global_rows_and_preserves_ties_and_last_value_duplicates() {
    let rows = index().suggest(&query("aa", false, 50)).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.tag.as_str()).collect::<Vec<_>>(),
        ["f:aa", "m:aa", "f:big aa"]
    );
    assert_eq!(rows[0].score, 2.0 / 9.0);
    assert_eq!(
        index().suggest(&query("fe", true, 50)).unwrap()[0].tag,
        "female:"
    );
    let duplicate =
        r#"{"f":{"bb":"first","aa":"x","bb":"last"},"m":{"bb":"m"},"f":{"dd":"d","cc":"c"}}"#;
    let index = TagIndex::parse(duplicate.as_bytes(), false).unwrap();
    assert_eq!(index.translation("f", "bb"), None);
    assert_eq!(index.suggest(&query("", false, 50)).unwrap(), []);
    let same = TagIndex::parse(
        r#"{"f":{"zz":"hint","aa":"hint","zz":"last"}}"#.as_bytes(),
        false,
    )
    .unwrap();
    assert_eq!(same.translation("f", "zz"), Some("last"));
    assert_eq!(
        same.suggest(&query("f:h", true, 50)).unwrap()[0].tag,
        "f:aa"
    );
    let tied = TagIndex::parse(r#"{"f":{"zz":"hint","aa":"hint"}}"#.as_bytes(), false).unwrap();
    assert_eq!(tied.suggest(&query("f:h", true, 1)).unwrap()[0].tag, "f:zz");
}
#[test]
fn handles_case_utf16_lengths_unknown_namespaces_and_all_weights() {
    let names = [
        "rows",
        "artist",
        "cosplayer",
        "character",
        "female",
        "group",
        "language",
        "location",
        "male",
        "mixed",
        "other",
        "parody",
        "reclass",
    ];
    let data = names
        .iter()
        .map(|name| {
            format!("{{\"namespace\":\"{name}\",\"data\":{{\"Aa😀\":{{\"name\":\"hint\"}}}}}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let index = TagIndex::parse(format!("{{\"data\":[{data}]}}").as_bytes(), true).unwrap();
    let rows = index.suggest(&query("AA", false, 50)).unwrap();
    assert_eq!(rows.len(), names.len());
    assert_eq!(rows[0].tag, "Aa😀:");
    assert_eq!(
        index.suggest(&query("artist:aa", false, 50)).unwrap()[0].score,
        4.0
    );
    assert!(
        index
            .suggest(&query("absent:AA", false, 50))
            .unwrap()
            .is_empty()
    );
    assert!(index.suggest(&query("n:AA", false, 50)).unwrap().is_empty());
    assert_eq!(simple_fold('ß'), 'ß');
    assert!(starts_ignore_case("İST", "ist"));
    assert!(starts_ignore_case("Σ", "ς"));
    assert!(!starts_ignore_case("a", "aa"));
}
#[test]
fn empty_and_malformed_inputs_are_fixed_body_free_failures() {
    assert!(
        TagIndex::parse(b"{}".as_slice(), false)
            .unwrap()
            .suggest(&query("x", true, 50))
            .unwrap()
            .is_empty()
    );
    for input in ["", "[]", "null", "{", "{\"f\":null}", "{\"f\":{\"x\":[]}}"] {
        assert_eq!(
            TagIndex::parse(input.as_bytes(), false).unwrap_err(),
            invalid()
        );
    }
    for input in [
        "{}",
        "{\"data\":[{\"namespace\":\"secret-unknown\",\"data\":{}}]}",
        "{\"data\":[{\"namespace\":\"female\",\"data\":{\"x\":{}}}]}",
    ] {
        assert_eq!(
            TagIndex::parse(input.as_bytes(), true).unwrap_err(),
            invalid()
        );
    }
    assert!(
        index()
            .suggest(&SuggestionRequest {
                version: 2,
                ..query("x", false, 50)
            })
            .is_err()
    );
    assert!(index().suggest(&query("x", false, 51)).is_err());
    assert!(
        index()
            .suggest(&query(&"x".repeat(16 * 1024 + 1), false, 50))
            .is_err()
    );
}
#[test]
fn remote_duplicates_and_field_budgets_preserve_the_existing_index_contract() {
    let raw = r#"{"data":[{"namespace":"female","data":{"aa":{"name":"old"}}},{"namespace":"female","data":{"bb":{"name":"new"}}}]}"#;
    let index = TagIndex::parse(raw.as_bytes(), true).unwrap();
    assert_eq!(index.translation("f", "aa"), None);
    assert_eq!(index.translation("f", "bb"), Some("new"));
    for source in [
        format!("{{\"{}\":{{}}}}", "x".repeat(129)),
        format!("{{\"f\":{{\"{}\":\"hint\"}}}}", "x".repeat(4097)),
        format!("{{\"f\":{{\"x\":\"{}\"}}}}", "x".repeat(64 * 1024 + 1)),
    ] {
        assert_eq!(
            TagIndex::parse(source.as_bytes(), false).unwrap_err(),
            invalid()
        );
    }
}
#[test]
fn exact_shared_cbor_goldens_match_kotlin() {
    use eh_domain::contracts::ApiEnvelope;
    let query = query("f:aa", true, 15);
    let index = TagIndex::parse(
        include_bytes!("../../test_data/tag_translation.json").as_slice(),
        false,
    )
    .unwrap();
    let hex = |bytes: Vec<u8>| {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut result, byte| {
            write!(result, "{byte:02x}").unwrap();
            result
        })
    };
    assert_eq!(
        hex(serde_cbor::to_vec(&query).unwrap()),
        include_str!("../../test_data/tag_translation_request.cbor.hex").trim()
    );
    assert_eq!(
        hex(
            serde_cbor::to_vec(&ApiEnvelope::ok(1, MODULE, index.suggest(&query).unwrap()))
                .unwrap()
        ),
        include_str!("../../test_data/tag_translation_response.cbor.hex").trim()
    );
}
#[test]
fn request_and_response_cbor_contract_round_trip() {
    let request = query("f:aa", true, 15);
    let bytes = serde_cbor::to_vec(&request).unwrap();
    let restored: SuggestionRequest = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(restored.keyword, request.keyword);
    let rows = index().suggest(&restored).unwrap();
    let restored: Vec<TagSuggestion> =
        serde_cbor::from_slice(&serde_cbor::to_vec(&rows).unwrap()).unwrap();
    assert_eq!(restored, rows);
}
