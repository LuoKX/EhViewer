use super::*;

fn utf16(value: &str) -> ByteBuf {
    value
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
        .into()
}

fn child(id: &str, name: &str, size: Option<i64>, modified: Option<i64>) -> Child {
    Child {
        source_id_utf16: utf16(id),
        name_utf16: utf16(name),
        size,
        modified_at_millis: modified,
    }
}

fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation,
        children: Vec::new(),
        promotions: Vec::new(),
    }
}

fn golden_request(operation: Operation) -> Request {
    let mut input = request(operation);
    match operation {
        Operation::Observe => {
            input.children = vec![
                child("b", "ComicInfo.xml", Some(3), Some(-2)),
                child("a", ".EHVIEWER", Some(4), Some(5)),
                child("a", ".ehviewer", None, None),
            ];
        }
        Operation::Promote => {
            input.promotions = vec![
                Promotion {
                    relative_path_utf16: utf16("dir/42.CBZ"),
                    kind: RecoveryFileKind::Cbz,
                    gid: Some(42),
                },
                Promotion {
                    relative_path_utf16: utf16("folder"),
                    kind: RecoveryFileKind::Directory,
                    gid: Some(7),
                },
                Promotion {
                    relative_path_utf16: utf16("7.zip"),
                    kind: RecoveryFileKind::Zip,
                    gid: Some(7),
                },
            ];
        }
    }
    input
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").unwrap();
        output
    })
}

#[test]
fn shared_exact_cbor_goldens() {
    for (operation, name) in [
        (Operation::Observe, "observe"),
        (Operation::Promote, "promote"),
    ] {
        let input = golden_request(operation);
        let output = build_envelope(&input);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");
        for (suffix, bytes) in [
            ("request", serde_cbor::to_vec(&input).unwrap()),
            ("response", serde_cbor::to_vec(&output).unwrap()),
            (
                "plan",
                serde_cbor::to_vec(output.payload.as_ref().unwrap()).unwrap(),
            ),
        ] {
            let path = root.join(format!("recovery_directory_{name}_{suffix}.cbor.hex"));
            // Fixture generation is opt-in, never part of production or CI.
            if std::env::var_os("UPDATE_RECOVERY_DIRECTORY_GOLDENS").is_some() {
                std::fs::write(&path, format!("{}\n", hex(&bytes))).unwrap();
            }
            assert_eq!(hex(&bytes), std::fs::read_to_string(path).unwrap().trim());
        }
        let decoded: Request =
            serde_cbor::from_slice(&serde_cbor::to_vec(&input).unwrap()).unwrap();
        assert_eq!(plan(&decoded).unwrap(), output.payload.unwrap());
    }
}

#[test]
fn stable_utf16_source_order_selects_first_sidecars() {
    let mut input = request(Operation::Observe);
    input.children = vec![
        child("\u{e000}", ".ehviewer", None, None),
        child("😀", ".EHVIEWER", None, None),
        child("😀", ".ehviewer", None, None),
        child("z", "COMİCİNFO.XML", Some(0), Some(-9)),
        child("a", "other", Some(-5), Some(-10)),
        child("z", "ComicInfo.xml", None, None),
    ];
    let output = plan(&input).unwrap();
    assert_eq!(output.indices, [4, 3, 5, 1, 2, 0]);
    assert_eq!(output.spider_index, Some(1));
    assert_eq!(output.comic_index, Some(3));
    assert_eq!(output.total_size, -5);
    assert_eq!(output.modified_at_millis, Some(-9));
    input.children[0].source_id_utf16 = vec![0, 0xd8].into(); // Lone surrogate sorts before pair.
    assert_eq!(plan(&input).unwrap().indices, [4, 3, 5, 0, 1, 2]);
}

#[test]
fn empty_missing_null_negative_and_wrapping_metadata() {
    let mut input = request(Operation::Observe);
    let empty = plan(&input).unwrap();
    assert!(empty.indices.is_empty());
    assert_eq!(empty.spider_index, None);
    assert_eq!(empty.comic_index, None);
    assert_eq!(empty.total_size, 0);
    assert_eq!(empty.modified_at_millis, None);
    input.children = vec![
        child("a", "", Some(i64::MAX), None),
        child("b", "unrelated", Some(1), Some(i64::MIN)),
    ];
    let overflow = plan(&input).unwrap();
    assert_eq!(overflow.total_size, i64::MIN);
    assert_eq!(overflow.modified_at_millis, Some(i64::MIN));
    // Legacy Kotlin already sent this negative wrapping total to the unsigned
    // candidate fingerprint codec, which rejects it. Do not silently repair it.
    let fingerprint = std::collections::BTreeMap::from([("size", overflow.total_size)]);
    assert!(
        serde_cbor::from_slice::<crate::recovery::RecoveryFingerprintInput>(
            &serde_cbor::to_vec(&fingerprint).unwrap()
        )
        .is_err()
    );
    input.children[0].size = Some(i64::MIN);
    input.children[1].size = Some(-1);
    input.children[1].modified_at_millis = Some(0);
    assert_eq!(plan(&input).unwrap().total_size, i64::MAX);
    assert_eq!(plan(&input).unwrap().modified_at_millis, Some(0));
    input.children = vec![
        child("a", ".ehvıewer", None, None),
        child("b", "comicinfo.xml.bak", None, None),
    ];
    assert_eq!(plan(&input).unwrap().spider_index, Some(0));
    assert_eq!(plan(&input).unwrap().comic_index, None);
}

#[test]
fn promotion_characterization_all_closed_kinds_gid_parent_and_case() {
    use PathDecision::{None as NoPath, Parent, Relative};
    use RecoveryFileKind::{Cbz, Directory, Rar, SevenZip, Tar, TarGz, TarXz, Zip};
    let cases = [
        ("dir", Directory, Some(1), Relative, 3),
        ("dir/", Directory, Some(1), Relative, 4),
        ("", Directory, Some(1), NoPath, 0),
        ("\u{00a0}\u{2007}\u{202f}", Directory, Some(1), NoPath, 0),
        ("\u{0085}", Directory, Some(1), Relative, 1),
        ("dir", Directory, None, NoPath, 0),
        ("dir", Directory, Some(0), NoPath, 0),
        ("dir", Directory, Some(-1), NoPath, 0),
        ("dir/42.cBz", Cbz, Some(42), Parent, 3),
        ("dir/42.ZİP", Zip, Some(42), Parent, 3),
        ("😀/42.zip", Zip, Some(42), Parent, 2),
        ("42.zip", Zip, Some(42), NoPath, 0),
        ("/42.zip", Zip, Some(42), NoPath, 0),
        (" /42.zip", Zip, Some(42), NoPath, 0),
        ("dir/43.zip", Zip, Some(42), NoPath, 0),
        ("dir/42.cbz", Zip, Some(42), NoPath, 0),
        ("dir/42.zip/", Zip, Some(42), NoPath, 0),
        (
            "a/b/9223372036854775807.CBZ",
            Cbz,
            Some(i64::MAX),
            Parent,
            3,
        ),
        ("dir/42.rar", Rar, Some(42), NoPath, 0),
        ("dir/42.7z", SevenZip, Some(42), NoPath, 0),
        ("dir/42.tar", Tar, Some(42), NoPath, 0),
        ("dir/42.tar.gz", TarGz, Some(42), NoPath, 0),
        ("dir/42.tar.xz", TarXz, Some(42), NoPath, 0),
    ];
    let mut input = request(Operation::Promote);
    input.promotions = cases
        .iter()
        .map(|(path, kind, gid, _, _)| Promotion {
            relative_path_utf16: utf16(path),
            kind: *kind,
            gid: *gid,
        })
        .collect();
    let output = plan(&input).unwrap();
    for ((_, _, _, decision, end), actual) in cases.iter().zip(output.promotions) {
        assert_eq!(actual.decision, *decision);
        assert_eq!(actual.end, *end);
    }
    assert!(
        plan(&request(Operation::Promote))
            .unwrap()
            .promotions
            .is_empty()
    );
}

#[test]
fn rejects_versions_counts_irrelevant_fields_and_text_budgets_with_fixed_errors() {
    let mut input = request(Operation::Observe);
    input.version = 2;
    assert_eq!(
        plan(&input).unwrap_err(),
        "Invalid recovery directory request"
    );
    assert!(build_envelope(&input).error.is_some());
    input.version = 1;
    input.children = (0..=MAX_ITEMS).map(|_| child("", "", None, None)).collect();
    assert!(plan(&input).is_err());
    input.children.clear();
    input.promotions = (0..=MAX_ITEMS)
        .map(|_| Promotion {
            relative_path_utf16: ByteBuf::new(),
            kind: RecoveryFileKind::Directory,
            gid: None,
        })
        .collect();
    input.operation = Operation::Promote;
    assert!(plan(&input).is_err());
    input.promotions.truncate(1);
    input
        .children
        .push(child("secret-id", "secret-name", None, None));
    assert_eq!(
        plan(&input).unwrap_err(),
        "Invalid recovery directory request"
    );
    input.operation = Operation::Observe;
    assert!(plan(&input).is_err());
    input.promotions.clear();
    input.children[0].name_utf16 = vec![1].into();
    assert_eq!(
        plan(&input).unwrap_err(),
        "Invalid recovery directory observations"
    );
    input.children[0].name_utf16 = vec![0; MAX_TEXT_BYTES].into();
    assert!(plan(&input).is_err());
    input.children[0].source_id_utf16 = ByteBuf::new();
    assert!(plan(&input).is_ok());
    input.children.push(child("x", "", None, None));
    assert!(plan(&input).is_err());
}

#[test]
fn malformed_cbor_and_unknown_closed_enums_fail() {
    assert!(serde_cbor::from_slice::<Request>(&[0xff]).is_err());
    let value = serde_cbor::Value::Map(std::collections::BTreeMap::from([
        (
            serde_cbor::Value::Text("version".into()),
            serde_cbor::Value::Integer(1),
        ),
        (
            serde_cbor::Value::Text("operation".into()),
            serde_cbor::Value::Text("secret-operation".into()),
        ),
        (
            serde_cbor::Value::Text("children".into()),
            serde_cbor::Value::Array(Vec::new()),
        ),
        (
            serde_cbor::Value::Text("promotions".into()),
            serde_cbor::Value::Array(Vec::new()),
        ),
    ]));
    assert!(serde_cbor::from_slice::<Request>(&serde_cbor::to_vec(&value).unwrap()).is_err());
    assert!(
        serde_cbor::from_slice::<RecoveryFileKind>(&serde_cbor::to_vec(&"unknown").unwrap())
            .is_err()
    );
    assert!(
        serde_cbor::from_slice::<PathDecision>(&serde_cbor::to_vec(&"unknown").unwrap()).is_err()
    );
}
