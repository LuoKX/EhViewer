use super::*;

fn request() -> BackupImportRequest {
    BackupImportRequest {
        version: 1,
        existing_gids: vec![],
        downloads: vec![],
        current_labels: vec![],
        labels: vec![],
    }
}
fn label(name: &str, position: i32) -> ImportLabel {
    ImportLabel {
        label_utf16: name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
        position,
    }
}
fn plan(request: &BackupImportRequest) -> BackupImportPlan {
    build_envelope(request).payload.expect("success")
}
fn hex(value: &str) -> Vec<u8> {
    value
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn shared_exact_cbor_goldens_match_independent_encoding_and_result() {
    let input = hex(include_str!(
        "../../test_data/backup_import_request.cbor.hex"
    ));
    let output = hex(include_str!(
        "../../test_data/backup_import_response.cbor.hex"
    ));
    let request: BackupImportRequest = serde_cbor::from_slice(&input).unwrap();
    assert_eq!(serde_cbor::to_vec(&request).unwrap(), input);
    let envelope = build_envelope(&request);
    assert_eq!(serde_cbor::to_vec(&envelope).unwrap(), output);
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<BackupImportPlan>>(&output).unwrap(),
        envelope
    );
    assert_eq!(
        plan(&request)
            .downloads
            .iter()
            .map(|row| (row.source_index, row.gid, row.state))
            .collect::<Vec<_>>(),
        vec![(1, 2, 0), (3, -9, 0), (4, 3, i32::MIN)]
    );
    assert_eq!(
        plan(&request).labels,
        vec![
            ImportLabelAddition {
                source_index: 1,
                position: 10
            },
            ImportLabelAddition {
                source_index: 3,
                position: 11
            }
        ]
    );
}

#[test]
fn excludes_existing_gids_and_keeps_first_surviving_duplicate_without_mutation() {
    let mut input = request();
    input.existing_gids = vec![7, 7, i64::MIN];
    input.downloads = vec![
        ImportDownload { gid: 7, state: 2 },
        ImportDownload { gid: 9, state: 1 },
        ImportDownload { gid: 9, state: 4 },
        ImportDownload {
            gid: i64::MIN,
            state: 0,
        },
        ImportDownload {
            gid: i64::MAX,
            state: i32::MAX,
        },
    ];
    let before = input.clone();
    let result = plan(&input);
    assert_eq!(
        result.downloads,
        vec![
            ImportDownloadAddition {
                source_index: 1,
                gid: 9,
                state: 0
            },
            ImportDownloadAddition {
                source_index: 4,
                gid: i64::MAX,
                state: i32::MAX
            }
        ]
    );
    assert_eq!(input, before);
}

#[test]
fn preserves_all_nonvolatile_signed_states() {
    let mut input = request();
    input.downloads = [i32::MIN, -1, 0, 1, 2, 3, 4, i32::MAX]
        .into_iter()
        .enumerate()
        .map(|(index, state)| ImportDownload {
            gid: i64::try_from(index).unwrap(),
            state,
        })
        .collect();
    assert_eq!(
        plan(&input)
            .downloads
            .iter()
            .map(|row| row.state)
            .collect::<Vec<_>>(),
        vec![i32::MIN, -1, 0, 0, 0, 3, 4, i32::MAX]
    );
}

#[test]
fn appends_first_new_names_after_sparse_maximum_in_input_order() {
    let mut input = request();
    input.current_labels = vec![label("old", 3), label("max", 9), label("old", -4)];
    input.labels = vec![
        label("old", 100),
        label("first", i32::MAX),
        label("first", i32::MIN),
        label("second", -5),
    ];
    let before = input.clone();
    assert_eq!(
        plan(&input).labels,
        vec![
            ImportLabelAddition {
                source_index: 1,
                position: 10
            },
            ImportLabelAddition {
                source_index: 3,
                position: 11
            }
        ]
    );
    assert_eq!(input, before);
}

#[test]
fn negative_and_empty_positions_start_at_zero_and_extreme_empty_imports_succeed() {
    let mut input = request();
    assert_eq!(
        plan(&input),
        BackupImportPlan {
            downloads: vec![],
            labels: vec![]
        }
    );
    for position in [i32::MIN, -9, -1] {
        input.current_labels = vec![label("old", position)];
        input.labels = vec![label("new", i32::MAX)];
        assert_eq!(plan(&input).labels[0].position, 0);
    }
    for position in [i32::MIN, i32::MAX] {
        input.current_labels = vec![label("old", position)];
        input.labels.clear();
        assert!(plan(&input).labels.is_empty());
        input.labels.push(label("old", 0));
        assert!(plan(&input).labels.is_empty());
    }
    input.current_labels.clear();
    assert_eq!(plan(&input).labels[0].position, 0);
}

#[test]
fn overflow_is_atomic_and_last_int_position_is_allowed() {
    let mut input = request();
    input.current_labels = vec![label("old", i32::MAX - 1)];
    input.labels = vec![label("new", 0)];
    assert_eq!(plan(&input).labels[0].position, i32::MAX);
    input.labels.push(label("second", 0));
    input.downloads.push(ImportDownload { gid: 7, state: 1 });
    let before = input.clone();
    let error = build_envelope(&input);
    assert!(error.payload.is_none());
    assert_eq!(
        error.error,
        Some(RustError::Error(
            "Download label positions exceed Int range".into()
        ))
    );
    assert_eq!(input, before);
    input.current_labels[0].position = i32::MAX;
    input.labels.truncate(1);
    assert!(build_envelope(&input).payload.is_none());
}

#[test]
fn exact_utf16_equality_preserves_case_empty_and_lone_surrogates() {
    let mut input = request();
    input.current_labels = vec![label("a", 0)];
    input.labels = vec![
        label("A", 0),
        label("", 0),
        ImportLabel {
            label_utf16: vec![0, 0xd8].into(),
            position: 0,
        },
        ImportLabel {
            label_utf16: vec![0, 0xdc].into(),
            position: 0,
        },
        label("a", 0),
    ];
    assert_eq!(plan(&input).labels.len(), 4);
    assert_eq!(
        plan(&input)
            .labels
            .iter()
            .map(|row| row.position)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn invalid_version_count_label_and_aggregate_budgets_are_fixed_redacted_errors() {
    let mut bad = vec![request()];
    bad[0].version = 2;
    let mut count = request();
    count.existing_gids = vec![0; MAX_ITEMS + 1];
    bad.push(count);
    let mut odd = request();
    odd.labels = vec![ImportLabel {
        label_utf16: vec![1].into(),
        position: 0,
    }];
    bad.push(odd);
    let mut length = request();
    length.current_labels = vec![ImportLabel {
        label_utf16: vec![0; MAX_LABEL_BYTES + 2].into(),
        position: 0,
    }];
    bad.push(length);
    let mut total = request();
    total.labels = vec![
        ImportLabel {
            label_utf16: vec![0; MAX_LABEL_BYTES].into(),
            position: 0
        };
        MAX_LABEL_TOTAL_BYTES / MAX_LABEL_BYTES + 1
    ];
    bad.push(total);
    for input in bad {
        let error = build_envelope(&input);
        assert!(error.payload.is_none());
        assert_eq!(
            error.error,
            Some(RustError::Error("Invalid backup import input".into()))
        );
    }
}

#[test]
fn exact_count_and_label_budgets_are_accepted() {
    let mut input = request();
    input.existing_gids = vec![0; MAX_ITEMS];
    assert!(build_envelope(&input).is_ok());
    input.existing_gids.clear();
    input.current_labels = vec![
        ImportLabel {
            label_utf16: vec![0; MAX_LABEL_BYTES].into(),
            position: 0
        };
        MAX_LABEL_TOTAL_BYTES / MAX_LABEL_BYTES
    ];
    assert!(build_envelope(&input).is_ok());
}

#[test]
fn malformed_wire_unknown_fields_and_integer_widths_are_rejected() {
    assert!(serde_cbor::from_slice::<BackupImportRequest>(&[0xff]).is_err());
    let bytes = hex(include_str!(
        "../../test_data/backup_import_request.cbor.hex"
    ));
    let mut value: serde_cbor::Value = serde_cbor::from_slice(&bytes).unwrap();
    if let serde_cbor::Value::Map(ref mut fields) = value {
        fields.insert(
            serde_cbor::Value::Text("unknown-secret".into()),
            serde_cbor::Value::Null,
        );
    }
    assert!(
        serde_cbor::from_slice::<BackupImportRequest>(&serde_cbor::to_vec(&value).unwrap())
            .is_err()
    );
    assert!(serde_json::from_str::<ImportDownload>(r#"{"gid":0,"state":2147483648}"#).is_err());
}
