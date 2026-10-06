use super::*;

mod legacy;

fn raw(text: &str) -> ByteBuf {
    text.encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
        .into()
}
fn request(operation: StorageOperation) -> StorageRequest {
    StorageRequest {
        version: 1,
        operation,
        observations: vec![],
        candidates: vec![],
        names: vec![],
        modified: vec![],
        max_items: 0,
    }
}
fn observation(index: u32, name: &str) -> FileObservation {
    let units: Vec<_> = name.encode_utf16().collect();
    FileObservation {
        index,
        directory: false,
        root: false,
        has_strict_page: false,
        suffix_utf16: units[units.len().saturating_sub(7)..]
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect::<Vec<_>>()
            .into(),
    }
}

#[test]
fn classification_is_extension_exact_and_keeps_platform_page_observations() {
    use RecoveryFileKind::*;
    let samples = [
        ("gallery.TAR.GZ", Some(TarGz)),
        ("gallery.tar.xz", Some(TarXz)),
        ("gallery.CBZ", Some(Cbz)),
        ("gallery.ZIP", Some(Zip)),
        ("gallery.RAR", Some(Rar)),
        ("gallery.7Z", Some(SevenZip)),
        ("gallery.tar", Some(Tar)),
        ("archivezip", None),
        ("archive.zip.tmp", None),
        ("gallery.gz", None),
        ("gallery.zip\n", None),
        (".tar.xz", Some(TarXz)),
        ("gallery.zİp", None),
        ("gallery.zıp", None),
        ("😀.ZIP", Some(Zip)),
        ("", None),
    ];
    let mut req = request(StorageOperation::Classify);
    req.observations = samples
        .iter()
        .enumerate()
        .map(|(index, (name, _))| observation(u32::try_from(index).unwrap(), name))
        .collect();
    let result = plan(&req).unwrap();
    assert_eq!(
        result.candidates,
        samples
            .iter()
            .enumerate()
            .filter_map(|(index, (_, kind))| kind.map(|kind| CandidateRef {
                index: u32::try_from(index).unwrap(),
                kind
            }))
            .collect::<Vec<_>>()
    );
    let mut dir = observation(1, "archive.zip");
    dir.directory = true;
    dir.has_strict_page = true;
    req.observations = vec![dir.clone()];
    assert_eq!(plan(&req).unwrap().candidates[0].kind, Directory);
    dir.root = true;
    req.observations = vec![dir.clone()];
    assert!(plan(&req).unwrap().candidates.is_empty());
    dir.root = false;
    dir.has_strict_page = false;
    req.observations = vec![dir];
    assert!(plan(&req).unwrap().candidates.is_empty());
}

#[test]
fn candidate_order_is_java_utf16_then_legacy_directory_first_and_snapshot_order() {
    let mut req = request(StorageOperation::Candidates);
    req.max_items = 3;
    req.candidates = vec![
        CandidateInput {
            index: 0,
            kind: RecoveryFileKind::Zip,
            source_id_utf16: raw("\u{e000}"),
        },
        CandidateInput {
            index: 1,
            kind: RecoveryFileKind::Zip,
            source_id_utf16: raw("same"),
        },
        CandidateInput {
            index: 2,
            kind: RecoveryFileKind::Directory,
            source_id_utf16: raw("same"),
        },
        CandidateInput {
            index: 3,
            kind: RecoveryFileKind::Tar,
            source_id_utf16: raw("same"),
        },
        CandidateInput {
            index: 4,
            kind: RecoveryFileKind::Zip,
            source_id_utf16: raw("😀"),
        },
    ];
    let result = plan(&req).unwrap();
    assert!(result.limit_exceeded);
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.index)
            .collect::<Vec<_>>(),
        [2, 1, 3]
    );
    req.max_items = 5;
    let result = plan(&req).unwrap();
    assert!(!result.limit_exceeded);
    assert_eq!(
        result
            .candidates
            .iter()
            .map(|candidate| candidate.index)
            .collect::<Vec<_>>(),
        [2, 1, 3, 4, 0]
    );
    req.max_items = 0;
    assert!(plan(&req).unwrap().candidates.is_empty());
    req.max_items = -1;
    assert!(plan(&req).is_err());
}

#[test]
fn physical_name_order_preserves_raw_surrogates_and_equal_input_order() {
    let mut req = request(StorageOperation::Names);
    req.names = [
        raw("\u{e000}"),
        raw("😀"),
        raw("a"),
        raw("a"),
        vec![0, 0xd8].into(),
        raw(""),
        raw("\u{100}"),
        raw("\u{ff}"),
    ]
    .into_iter()
    .map(|utf16| NameInput { utf16 })
    .collect();
    assert_eq!(plan(&req).unwrap().indices, [5, 2, 3, 7, 6, 4, 1, 0]);
}

#[test]
fn retention_is_stable_signed_and_never_subtracts_overflowing_timestamps() {
    let mut req = request(StorageOperation::Cleanup);
    req.modified = vec![i64::MIN, 10, i64::MAX, 10, -1, 0];
    for (limit, expected) in [
        (2, vec![3, 5, 4, 0]),
        (0, vec![2, 1, 3, 5, 4, 0]),
        (-3, vec![2, 1, 3, 5, 4, 0]),
        (6, vec![]),
        (i32::MAX, vec![]),
    ] {
        req.max_items = limit;
        assert_eq!(plan(&req).unwrap().indices, expected);
    }
    req.modified.clear();
    assert!(plan(&req).unwrap().indices.is_empty());
}

#[test]
fn every_wire_operation_roundtrips_and_empty_requests_are_valid() {
    for op in [
        StorageOperation::Classify,
        StorageOperation::Candidates,
        StorageOperation::Names,
        StorageOperation::Cleanup,
    ] {
        let req = request(op);
        let encoded = serde_cbor::to_vec(&req).unwrap();
        let decoded: StorageRequest = serde_cbor::from_slice(&encoded).unwrap();
        assert_eq!(decoded.operation, op);
        let envelope = build_envelope(&decoded);
        assert!(envelope.is_ok());
        let encoded = serde_cbor::to_vec(&envelope).unwrap();
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<StoragePlan>>(&encoded).unwrap(),
            envelope
        );
    }
}

#[test]
fn closed_shapes_raw_bytes_slots_and_resource_ceilings_are_enforced() {
    let base = request(StorageOperation::Classify);
    let mut bad = base.clone();
    bad.version = 2;
    assert!(build_envelope(&bad).error.is_some());
    let mut bad = base.clone();
    bad.max_items = 1;
    assert!(plan(&bad).is_err());
    let mut bad = base.clone();
    bad.modified = vec![0];
    assert!(plan(&bad).is_err());
    let mut bad = base.clone();
    bad.names = vec![NameInput { utf16: raw("a") }];
    assert!(plan(&bad).is_err());
    let candidate = CandidateInput {
        index: 0,
        kind: RecoveryFileKind::Zip,
        source_id_utf16: raw("source"),
    };
    let mut bad = base.clone();
    bad.candidates = vec![candidate.clone()];
    assert!(plan(&bad).is_err());
    let mut bad = base.clone();
    bad.observations = vec![observation(0, "a.zip"); MAX_CLASSIFY_ITEMS + 1];
    assert!(plan(&bad).is_err());
    let mut bad = request(StorageOperation::Cleanup);
    bad.modified = vec![0; MAX_ITEMS + 1];
    assert!(plan(&bad).is_err());
    let mut bad = request(StorageOperation::Names);
    bad.names = vec![NameInput { utf16: raw("") }; MAX_ITEMS + 1];
    assert!(plan(&bad).is_err());
    let mut bad = request(StorageOperation::Candidates);
    bad.candidates = vec![candidate; MAX_ITEMS + 1];
    assert!(plan(&bad).is_err());
    let mut bad = request(StorageOperation::Cleanup);
    bad.observations = vec![observation(0, "a.zip")];
    assert!(plan(&bad).is_err());
    for name in [vec![1], vec![0; MAX_NAME_BYTES + 2]] {
        let mut bad = request(StorageOperation::Names);
        bad.names = vec![NameInput { utf16: name.into() }];
        assert!(plan(&bad).is_err());
    }
    let mut bad = request(StorageOperation::Candidates);
    bad.candidates = vec![CandidateInput {
        index: u32::try_from(MAX_ITEMS).unwrap(),
        kind: RecoveryFileKind::Zip,
        source_id_utf16: raw("a"),
    }];
    assert!(plan(&bad).is_err());
    for (index, suffix) in [
        (u32::try_from(MAX_ITEMS).unwrap(), raw(".zip")),
        (0, vec![1].into()),
        (0, raw("12345678")),
    ] {
        let mut bad = base.clone();
        let mut item = observation(index, "");
        item.suffix_utf16 = suffix;
        bad.observations = vec![item];
        assert!(plan(&bad).is_err());
    }
    let mut bad = request(StorageOperation::Names);
    bad.max_items = 1;
    assert!(plan(&bad).is_err());
    let mut bad = request(StorageOperation::Names);
    bad.names = vec![
        NameInput {
            utf16: vec![0; MAX_NAME_BYTES].into(),
        },
        NameInput { utf16: raw("a") },
    ];
    assert!(plan(&bad).is_err());
}
