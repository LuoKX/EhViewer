use super::*;

fn info() -> SpiderInfo {
    SpiderInfo {
        gid: 7,
        token: "0123456789".into(),
        pages: 3,
        p_token_map: BTreeMap::from([(0, "a".into()), (2, "b".into())]),
        preview_per_page: 40,
    }
}
#[test]
fn complete_current_codec_defaults_unknown_fields_and_indefinite_maps() {
    let info = info();
    let bytes = encode(&info).unwrap();
    assert_eq!(decode(&bytes, false).unwrap(), info);
    assert_eq!(
        decode_identity(&bytes).unwrap(),
        (7, "0123456789".into(), 3)
    );
    let value = serde_json::json!({"gid":7,"token":"x","pages":3,"future":"ignored"});
    let bytes = serde_cbor::to_vec(&value).unwrap();
    let decoded = decode(&bytes, true).unwrap();
    assert_eq!(decoded.p_token_map, BTreeMap::new());
    assert_eq!(decoded.preview_per_page, -1);
    let bytes = encode(&decoded).unwrap();
    let value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
    let serde_cbor::Value::Map(map) = value else {
        panic!("storage map")
    };
    assert_eq!(map.len(), 3);
    let mut indefinite = vec![0xbf];
    indefinite.extend(&bytes[1..]);
    indefinite.push(0xff);
    assert_eq!(decode(&indefinite, false).unwrap(), decoded);
    let wire = serde_json::json!({"gid":7,"token":"x","pages":3,"pTokenMap":"ignored by recovery"});
    assert!(decode(&serde_cbor::to_vec(&wire).unwrap(), false).is_err());
    assert_eq!(
        decode_identity(&serde_cbor::to_vec(&wire).unwrap()).unwrap(),
        (7, "x".into(), 3)
    );
}
#[test]
fn legacy_partial_rows_duplicates_strict_newlines_and_recovery_compat() {
    let text = b"v2\n0\n7\n0123456789\nread\n1\n40\n3\n0 old\n0 new\nignore\n empty index\n1 \n2 kept\nbad fail\n1 ignored\n";
    let decoded = decode(text, true).unwrap();
    assert_eq!(
        decoded.p_token_map,
        BTreeMap::from([(0, "new".into()), (2, "kept".into())])
    );
    assert_eq!(decoded.preview_per_page, 40);
    assert!(decode(text, false).is_err());
    assert_eq!(decode_identity(text).unwrap(), (7, "0123456789".into(), 3));
    let crlf = String::from_utf8(text.to_vec())
        .unwrap()
        .replace('\n', "\r\n");
    assert_eq!(decode(crlf.as_bytes(), true).unwrap(), decoded);
    assert!(decode(b"v\nx\n7\nx\nx\nx\n-1\n3", true).is_err());
    assert_eq!(
        decode_identity(b"v\nx\n7\nx\nx\nx\n-1\n3").unwrap(),
        (7, "x".into(), 3)
    );
    assert!(decode(b"v\nx\nbad\nx\nx\nx\n-1\n3\n", true).is_err());
    assert!(decode(b"v\nx\n7\nx\nx\nx\nbad\n3\n", true).is_err());
    assert!(decode(b"v\nx\n7\nx\nx\nx\n-1\nbad\n", true).is_err());
    let decoded = decode(b"v\nx\n7\nx\nx\nx\n-1\n3\n2 last", true).unwrap();
    assert!(decoded.p_token_map.is_empty());
}
#[test]
fn bounds_errors_and_full_wire_results() {
    for bytes in [vec![], vec![0xff], vec![0xff; MAX_BYTES + 1], vec![b'x']] {
        assert!(decode(&bytes, true).is_err());
        assert!(decode_identity(&bytes).is_err());
    }
    let mut oversized = info();
    oversized.token = "x".repeat(MAX_BYTES);
    assert!(encode(&oversized).is_err());
    for request in [
        SpiderInfoRequest::Encode { info: info() },
        SpiderInfoRequest::Decode {
            bytes: encode(&info()).unwrap().into(),
            allow_legacy: true,
        },
    ] {
        let result = build_envelope(1, &request);
        assert!(result.is_ok());
        assert!(!build_envelope(2, &request).is_ok());
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<SpiderInfoPlan>>(
                &serde_cbor::to_vec(&result).unwrap()
            )
            .unwrap(),
            result
        );
        let _: SpiderInfoRequest =
            serde_cbor::from_slice(&serde_cbor::to_vec(&request).unwrap()).unwrap();
    }
    assert!(
        !build_envelope(
            1,
            &SpiderInfoRequest::Decode {
                bytes: vec![0xff].into(),
                allow_legacy: true
            }
        )
        .is_ok()
    );
}
