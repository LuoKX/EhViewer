//! Frozen shared storage/scan wire fixtures; host JNI and Kotlin consume exact bytes.
use eh_domain::{
    recovery_scan::{ScanBackend, ScanOptions},
    storage_rules::*,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_bytes::ByteBuf;
use std::{fmt::Write as _, path::Path};

fn fixture<T: Serialize + DeserializeOwned>(name: &str, value: &T) {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let hex = bytes.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").unwrap();
        output
    }) + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(format!("{name}.cbor.hex"));
    if std::env::var_os("UPDATE_P4_GOLDENS").is_some() {
        std::fs::write(&path, &hex).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), hex);
    let decoded: T = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(serde_cbor::to_vec(&decoded).unwrap(), bytes);
}
fn raw(value: &str) -> ByteBuf {
    value
        .encode_utf16()
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
#[test]
fn storage_plans_and_scan_options_are_frozen() {
    let mut classify = request(StorageOperation::Classify);
    for (index, name) in [
        ".tar.gz", ".tar.xz", ".CBZ", ".ZIP", ".RAR", ".7Z", ".tar", "zip.tmp",
    ]
    .into_iter()
    .enumerate()
    {
        classify.observations.push(FileObservation {
            index: u32::try_from(index).unwrap(),
            directory: false,
            root: false,
            has_strict_page: false,
            suffix_utf16: raw(name),
        });
    }
    classify.observations.push(FileObservation {
        index: 8,
        directory: true,
        root: false,
        has_strict_page: true,
        suffix_utf16: raw("gallery"),
    });
    classify.observations.push(FileObservation {
        index: 9,
        directory: true,
        root: true,
        has_strict_page: true,
        suffix_utf16: raw("root"),
    });
    let mut candidates = request(StorageOperation::Candidates);
    candidates.max_items = 3;
    candidates.candidates = vec![
        CandidateInput {
            index: 0,
            kind: RecoveryFileKind::Zip,
            source_id_utf16: raw("\u{e000}"),
        },
        CandidateInput {
            index: 1,
            kind: RecoveryFileKind::Cbz,
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
            source_id_utf16: vec![0, 0xd8].into(),
        },
    ];
    let mut names = request(StorageOperation::Names);
    names.names = [
        raw("\u{e000}"),
        raw("😀"),
        raw("a"),
        vec![0, 0xd8].into(),
        raw("a"),
    ]
    .into_iter()
    .map(|utf16| NameInput { utf16 })
    .collect();
    let mut cleanup = request(StorageOperation::Cleanup);
    cleanup.modified = vec![i64::MIN, 10, i64::MAX, 10, 0];
    cleanup.max_items = 2;
    for (name, request) in [
        ("classify", classify),
        ("candidates", candidates),
        ("names", names),
        ("cleanup", cleanup),
    ] {
        fixture(&format!("storage_{name}_request"), &request);
        fixture(
            &format!("storage_{name}_response"),
            &build_envelope(&request),
        );
    }
    for (name, backend, root) in [
        ("physical", ScanBackend::Physical, vec![]),
        ("documents", ScanBackend::Documents, vec![b'r', 0, 0, 0xd8]),
    ] {
        fixture(
            &format!("recovery_scan_{name}_open"),
            &ScanOptions {
                version: 1,
                backend,
                max_depth: 1,
                max_nodes: 4,
                started_at_millis: 0,
                max_duration_millis: 1_000_000,
                root_document_id: root.into(),
            },
        );
    }
}
