use super::*;

fn utf16(text: &str) -> ByteBuf {
    text.encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
        .into()
}

fn candidate(gid: Option<i64>, status: RecoveryStatus, key: char, path: Option<&str>) -> Candidate {
    Candidate {
        gid,
        status,
        source_key: key.to_string().repeat(64),
        path_utf16: path.map(utf16),
    }
}

fn request() -> Request {
    Request {
        version: 1,
        candidates: vec![],
        downloads: vec![],
    }
}

#[test]
fn preserves_first_ready_match_nullable_path_and_download_order() {
    let mut input = request();
    input.candidates = vec![
        candidate(Some(7), RecoveryStatus::Incomplete, '0', Some("x")),
        candidate(None, RecoveryStatus::Ready, '1', Some("x")),
        candidate(Some(7), RecoveryStatus::Ready, 'a', Some("x")),
        candidate(Some(7), RecoveryStatus::Ready, 'b', Some("x")),
        candidate(Some(7), RecoveryStatus::Ready, 'c', None),
        candidate(Some(7), RecoveryStatus::Ready, 'd', Some("")),
    ];
    input.downloads = [Some("x"), None, Some(""), Some("X"), Some("x")]
        .into_iter()
        .map(|path| Download {
            gid: 7,
            dirname_utf16: path.map(utf16),
        })
        .collect();
    let plan = plan(&input).unwrap();
    assert_eq!(
        plan.mappings
            .iter()
            .map(|item| item.candidate_index)
            .collect::<Vec<_>>(),
        [Some(2), Some(4), Some(5), None, Some(2)]
    );
    for (index, key) in [(0, 'a'), (1, 'c'), (2, 'd'), (4, 'a')] {
        assert_eq!(plan.mappings[index].source_key, key.to_string().repeat(64));
    }
    assert_eq!(build_envelope(&request()).payload.unwrap().mappings, []);
}

#[test]
fn retains_unframed_hash_and_java_replacement_without_normalizing_matching() {
    let mut input = request();
    let malformed = ByteBuf::from(vec![0, 0xd8]);
    input.candidates = vec![Candidate {
        path_utf16: Some(malformed.clone()),
        ..candidate(Some(1), RecoveryStatus::Ready, 'a', None)
    }];
    input.downloads = vec![
        Download {
            gid: 1,
            dirname_utf16: Some(malformed),
        },
        Download {
            gid: 1,
            dirname_utf16: Some(utf16("?")),
        },
        Download {
            gid: 8,
            dirname_utf16: None,
        },
        Download {
            gid: 8,
            dirname_utf16: Some(utf16("")),
        },
        Download {
            gid: i64::MIN,
            dirname_utf16: Some(vec![0, 0xdc].into()),
        },
        Download {
            gid: 0,
            dirname_utf16: Some(vec![0x3d, 0xd8, 0, 0xde, 0, 0xd8, 0, 0xd8].into()),
        },
    ];
    let values = plan(&input).unwrap().mappings;
    assert_eq!(values[0].candidate_index, Some(0));
    assert_eq!(values[1].candidate_index, None);
    assert_eq!(values[2], values[3]);
    assert_eq!(
        values[2].source_key,
        "8fc478db3bc7cea0bfcd0fb1efde6ebf2f604aac0e2f855e94f39af9af80f811"
    );
    // Exact digest values are independently checked against MessageDigest by the host JNI oracle.
    assert_eq!(
        values[4].source_key,
        "b402303dbdc4ef7f5d0dbc16dfe859e11b0d5e31b8a9b427481d616071291600"
    );
    assert_eq!(
        values[5].source_key,
        "b857dbd0b3ffe686f260efa9178ea54d9c4eec89bdb0f93b3f973006a51bb778"
    );
}

#[test]
fn rejects_versions_counts_keys_and_path_budgets_without_input_in_errors() {
    let base = request();
    assert!(
        plan(&Request {
            version: 2,
            ..base.clone()
        })
        .is_err()
    );
    let c = candidate(Some(1), RecoveryStatus::Ready, 'a', None);
    assert!(
        plan(&Request {
            candidates: vec![c.clone(); MAX_ITEMS + 1],
            ..base.clone()
        })
        .is_err()
    );
    let d = Download {
        gid: 1,
        dirname_utf16: None,
    };
    assert!(
        plan(&Request {
            downloads: vec![d.clone(); MAX_ITEMS + 1],
            ..base.clone()
        })
        .is_err()
    );
    for key in ["secret-path".into(), "G".repeat(64), "a".repeat(63)] {
        let input = Request {
            candidates: vec![Candidate {
                source_key: key,
                ..c.clone()
            }],
            ..base.clone()
        };
        let response = build_envelope(&input);
        assert!(response.payload.is_none());
        assert_eq!(
            response.error,
            Some(RustError::Error("Invalid recovery mapping request".into()))
        );
    }
    for size in [1, MAX_TEXT_BYTES + 2] {
        assert!(
            plan(&Request {
                candidates: vec![Candidate {
                    path_utf16: Some(vec![0; size].into()),
                    ..c.clone()
                }],
                ..base.clone()
            })
            .is_err()
        );
        assert!(
            plan(&Request {
                downloads: vec![Download {
                    dirname_utf16: Some(vec![0; size].into()),
                    ..d.clone()
                }],
                ..base.clone()
            })
            .is_err()
        );
    }
    let max = vec![0; MAX_TEXT_BYTES];
    assert!(
        plan(&Request {
            candidates: vec![Candidate {
                path_utf16: Some(max.clone().into()),
                ..c.clone()
            }],
            ..base.clone()
        })
        .is_ok()
    );
    assert!(
        plan(&Request {
            candidates: vec![Candidate {
                path_utf16: Some(max.into()),
                ..c
            }],
            downloads: vec![Download {
                dirname_utf16: Some(vec![0, 0].into()),
                ..d
            }],
            ..base
        })
        .is_err()
    );
}

fn unhex(hex: &str) -> Vec<u8> {
    hex.trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn exact_shared_cbor_request_plan_and_envelope() {
    let bytes = unhex(include_str!(
        "../../test_data/recovery_mapping_request.cbor.hex"
    ));
    let input: Request = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(serde_cbor::to_vec(&input).unwrap(), bytes);
    let output = build_envelope(&input);
    assert_eq!(
        serde_cbor::to_vec(&output).unwrap(),
        unhex(include_str!(
            "../../test_data/recovery_mapping_response.cbor.hex"
        ))
    );
    assert_eq!(
        serde_cbor::to_vec(output.payload.as_ref().unwrap()).unwrap(),
        unhex(include_str!(
            "../../test_data/recovery_mapping_plan.cbor.hex"
        ))
    );
    assert!(serde_json::from_str::<Request>(r#"{"version":1,"candidates":[{"gid":1,"status":"unknown","sourceKey":"x","pathUtf16":null}],"downloads":[]}"#).is_err());
}
