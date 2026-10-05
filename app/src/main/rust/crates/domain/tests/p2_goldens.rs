//! Shared V1 request/response fixtures for the P2 portable owners.
use eh_domain::{local_pages, reader_policy, recovery_search, spider_info};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::Path;

fn bytes(name: &str) -> Vec<u8> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_data")
            .join(format!("{name}_request.cbor.hex")),
    )
    .unwrap();
    text.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn verify<T: Serialize>(name: &str, result: T) {
    let value = serde_cbor::to_vec(&result).unwrap();
    let expected = value.iter().fold(String::new(), |mut hex, byte| {
        write!(hex, "{byte:02x}").unwrap();
        hex
    }) + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(format!("{name}_response.cbor.hex"));
    if std::env::var_os("UPDATE_P2_GOLDENS").is_some() {
        std::fs::write(&path, &expected).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
}
#[test]
fn exact_shared_p2_response_goldens() {
    #[derive(Deserialize)]
    struct ReaderWire {
        version: u8,
        #[serde(flatten)]
        request: reader_policy::ReaderPolicyRequest,
    }
    #[derive(Deserialize)]
    struct SpiderWire {
        version: u8,
        #[serde(flatten)]
        request: spider_info::SpiderInfoRequest,
    }
    let request: ReaderWire = serde_cbor::from_slice(&bytes("reader_policy")).unwrap();
    verify(
        "reader_policy",
        reader_policy::build_envelope(request.version, &request.request),
    );
    let request: recovery_search::RecoverySearchRequest =
        serde_cbor::from_slice(&bytes("recovery_search")).unwrap();
    verify("recovery_search", recovery_search::build_envelope(&request));
    let request: local_pages::LocalPagesRequest =
        serde_cbor::from_slice(&bytes("local_pages")).unwrap();
    verify("local_pages", local_pages::build_envelope(&request));
    let request: SpiderWire = serde_cbor::from_slice(&bytes("spider_info")).unwrap();
    verify(
        "spider_info",
        spider_info::build_envelope(request.version, &request.request),
    );
}
