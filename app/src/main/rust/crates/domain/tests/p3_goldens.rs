//! Shared V1 filename and text-span fixtures.
use eh_domain::{file_rules, text_spans};
use serde::{Serialize, de::DeserializeOwned};
use std::fmt::Write as _;
use std::path::Path;

fn fixture<T: Serialize + DeserializeOwned + PartialEq>(name: &str, value: &T) {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let hex = bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    }) + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(format!("{name}.cbor.hex"));
    if std::env::var_os("UPDATE_P3_GOLDENS").is_some() {
        std::fs::write(&path, &hex).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), hex);
    assert!(serde_cbor::from_slice::<T>(&bytes).unwrap() == *value);
}
#[test]
fn exact_shared_requests_and_responses() {
    let filename = file_rules::FileRulesRequest {
        version: 1,
        kind: file_rules::FileRuleKind::Sanitize,
        units: Some("a:/中😀. ".encode_utf16().collect()),
    };
    fixture("file_rules_request", &filename);
    fixture(
        "file_rules_response",
        &file_rules::build_envelope(&filename),
    );
    let spans = text_spans::TextSpansRequest {
        version: 1,
        text_length: 12,
        spans: vec![
            text_spans::TextSpan {
                start: 3,
                end: 6,
                flags: 3,
            },
            text_spans::TextSpan {
                start: 0,
                end: 3,
                flags: 1,
            },
        ],
    };
    fixture("text_spans_request", &spans);
    fixture("text_spans_response", &text_spans::build_envelope(&spans));
}
