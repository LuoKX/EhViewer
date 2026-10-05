//! Shared V1 cache-key request/response fixtures.
use eh_net::cache_keys::{CacheKeyRequest, build_envelope};
use std::fmt::Write as _;
use std::path::Path;
#[test]
fn exact_shared_cache_key_response_golden() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");
    let text = std::fs::read_to_string(root.join("cache_key_request.cbor.hex")).unwrap();
    let bytes = text
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let request: CacheKeyRequest = serde_cbor::from_slice(&bytes).unwrap();
    let value = serde_cbor::to_vec(&build_envelope(&request))
        .unwrap()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02x}").unwrap();
            hex
        })
        + "\n";
    let path = root.join("cache_key_response.cbor.hex");
    if std::env::var_os("UPDATE_P2_GOLDENS").is_some() {
        std::fs::write(&path, &value).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), value);
}
