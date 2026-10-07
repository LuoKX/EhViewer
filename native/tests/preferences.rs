//! Focused std-only synthetic preference codec/runtime tests.
#![deny(missing_docs, unsafe_code, clippy::all, clippy::pedantic)]

#[path = "../src/preference_lock.rs"]
mod preference_lock;

#[path = "../src/preferences.rs"]
mod preferences;

use preferences::{
    CommitOutcome, Error, FixtureStore, MAX_BYTES, MAX_ENTRIES, MAX_SET_STRINGS, PreferenceSet,
    Preferences, Value,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

const ALL: &[u8] = include_bytes!("../parity/preferences/goldens/all-types.input.bin");
const EXTREMES: &[u8] = include_bytes!("../parity/preferences/goldens/extremes.input.bin");
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        // cwd-only synthetic directories: no root/global cache or app paths.
        let root = option_env!("CARGO_MANIFEST_DIR").map_or_else(
            || PathBuf::from("native/parity/preferences/build"),
            |manifest| Path::new(manifest).join("parity/preferences/build"),
        );
        let path = root.join(format!(
            "test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn data(&self) -> PathBuf {
        self.0.join("preferences.pb")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn set(strings: &[&str]) -> Value {
    Value::StringSet(PreferenceSet::from_strings(strings.iter().map(|s| (*s).to_owned())).unwrap())
}
fn wrap(value: &[u8]) -> Vec<u8> {
    let mut entry = vec![10, 1, b'x'];
    field(&mut entry, 18, value);
    let mut out = Vec::new();
    field(&mut out, 10, &entry);
    out
}
fn varint(out: &mut Vec<u8>, mut n: u64) {
    while n >= 128 {
        out.push((n.to_le_bytes()[0] & 127) | 128);
        n >>= 7;
    }
    out.push(n.to_le_bytes()[0]);
}
fn field(out: &mut Vec<u8>, tag: u8, bytes: &[u8]) {
    out.push(tag);
    varint(out, u64::try_from(bytes.len()).unwrap());
    out.extend_from_slice(bytes);
}

#[test]
fn cached_jvm_all_types_signed_bits_unicode_and_bytes() {
    let doc = Preferences::decode(ALL).unwrap();
    assert_eq!(doc.len(), 8);
    assert!(!doc.is_empty());
    assert!(!doc.is_read_only());
    for (key, value) in [
        ("a_bool", Value::Boolean(false)),
        ("b_int", Value::Integer(i32::MIN)),
        ("c_long", Value::Long(i64::MIN)),
        ("d_float", Value::FloatBits(0x7fc0_1234)),
        ("e_string", Value::String("雪\0😀".to_owned())),
        ("f_set", set(&["雪", "x"])),
        ("g_double", Value::DoubleBits(0xfff8_0000_0000_1234)),
        ("h_bytes", Value::Bytes(vec![0, 255, 128])),
    ] {
        assert_eq!(doc.get(key), Some(&value));
    }
    assert_eq!(doc.encode().unwrap(), ALL);
    // Rebuild via Rust encoder: deduplicate, but preserve first encounter order.
    let mut rebuilt = Preferences::default();
    for key in [
        "a_bool", "b_int", "c_long", "d_float", "e_string", "f_set", "g_double", "h_bytes",
    ] {
        rebuilt
            .set(key.to_owned(), doc.get(key).unwrap().clone())
            .unwrap();
    }
    assert_eq!(
        rebuilt.encode().unwrap(),
        include_bytes!("../parity/preferences/goldens/deduplicated-all-types.jvm.bin")
    );
    rebuilt.set("f_set".to_owned(), set(&["x", "雪"])).unwrap();
    assert_eq!(
        rebuilt.encode().unwrap(),
        include_bytes!("../parity/preferences/goldens/canonical-all-types.jvm.bin")
    );
    let doc = Preferences::decode(EXTREMES).unwrap();
    for (key, value) in [
        ("a_int", Value::Integer(i32::MAX)),
        ("b_long", Value::Long(i64::MAX)),
        ("c_minus", Value::Integer(-1)),
        ("d_float", Value::FloatBits(0x8000_0000)),
        ("e_double", Value::DoubleBits(0x7ff0_0000_0000_0000)),
    ] {
        assert_eq!(doc.get(key), Some(&value));
    }
    assert_eq!(doc.encode().unwrap(), EXTREMES);
}

#[test]
fn cached_jvm_duplicate_map_oneof_and_message_semantics() {
    for (bytes, key, expected) in [
        (
            include_bytes!("../parity/preferences/goldens/duplicate-map.input.bin").as_slice(),
            "x",
            Value::Integer(2),
        ),
        (
            include_bytes!("../parity/preferences/goldens/duplicate-oneof.input.bin").as_slice(),
            "x",
            Value::Boolean(true),
        ),
        (
            include_bytes!("../parity/preferences/goldens/merge-set.input.bin").as_slice(),
            "x",
            set(&["a", "b"]),
        ),
        (
            include_bytes!("../parity/preferences/goldens/replace-set.input.bin").as_slice(),
            "x",
            set(&["b"]),
        ),
        (
            include_bytes!("../parity/preferences/goldens/duplicate-entry-value.input.bin")
                .as_slice(),
            "x",
            set(&["b"]),
        ),
        (
            include_bytes!("../parity/preferences/goldens/duplicate-entry-key.input.bin")
                .as_slice(),
            "b",
            Value::Integer(1),
        ),
        (
            include_bytes!("../parity/preferences/goldens/missing-key.input.bin").as_slice(),
            "",
            Value::Integer(1),
        ),
        (
            include_bytes!("../parity/preferences/goldens/bool-noncanonical.input.bin").as_slice(),
            "x",
            Value::Boolean(true),
        ),
        (
            include_bytes!("../parity/preferences/goldens/int-truncation.input.bin").as_slice(),
            "x",
            Value::Integer(-1),
        ),
    ] {
        let doc = Preferences::decode(bytes).unwrap();
        assert_eq!(doc.get(key), Some(&expected));
        assert_eq!(doc.encode().unwrap(), bytes);
    }
    let empty = Preferences::decode(include_bytes!(
        "../parity/preferences/goldens/empty.input.bin"
    ))
    .unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.get("missing"), None);
    assert_eq!(empty.encode().unwrap(), []);
}

#[test]
fn cached_jvm_unset_map_entries_validate_only_effective_values() {
    for (bytes, key, read_only) in [
        (
            include_bytes!("../parity/preferences/goldens/duplicate-unset-valid.input.bin")
                .as_slice(),
            "x",
            false,
        ),
        (
            include_bytes!("../parity/preferences/goldens/repeated-empty-values-valid.input.bin")
                .as_slice(),
            "x",
            false,
        ),
        (
            include_bytes!("../parity/preferences/goldens/repeated-empty-entries-valid.input.bin")
                .as_slice(),
            "",
            false,
        ),
        (
            include_bytes!("../parity/preferences/goldens/duplicate-future-valid.input.bin")
                .as_slice(),
            "x",
            true,
        ),
        (
            include_bytes!("../parity/preferences/goldens/duplicate-unknown-unset-valid.input.bin")
                .as_slice(),
            "x",
            true,
        ),
    ] {
        let mut doc = Preferences::decode(bytes).unwrap();
        assert_eq!(doc.len(), 1);
        assert_eq!(doc.get(key), Some(&Value::Integer(1)));
        assert_eq!(doc.is_read_only(), read_only);
        assert_eq!(doc.encode().unwrap(), bytes);
        if read_only {
            assert_eq!(
                doc.set("unrelated".to_owned(), Value::Boolean(true)),
                Err(Error::UnknownFields)
            );
            assert_eq!(doc.remove(key), Err(Error::UnknownFields));
            assert_eq!(doc.encode().unwrap(), bytes);
        } else {
            doc.set(key.to_owned(), Value::Integer(2)).unwrap();
            assert_eq!(
                Preferences::decode(&doc.encode().unwrap())
                    .unwrap()
                    .get(key),
                Some(&Value::Integer(2))
            );
        }
    }
    let jvm = include_bytes!("../parity/preferences/goldens/duplicate-unset-valid.jvm.bin");
    assert_eq!(jvm.as_slice(), wrap(&[24, 1]));
    assert_eq!(
        Preferences::decode(jvm).unwrap().get("x"),
        Some(&Value::Integer(1))
    );
    // Effective ordered set remains ordered despite an earlier unset same-key entry.
    let mut entry = Vec::new();
    field(&mut entry, 10, b"f_set");
    let mut bytes = Vec::new();
    field(&mut bytes, 10, &entry);
    bytes.extend_from_slice(ALL);
    let doc = Preferences::decode(&bytes).unwrap();
    assert_eq!(doc.get("f_set"), Some(&set(&["雪", "x"])));
    assert_eq!(doc.encode().unwrap(), bytes);
}

#[test]
fn cached_jvm_effective_unset_values_still_reject() {
    for (bytes, jvm) in [
        (
            include_bytes!("../parity/preferences/goldens/duplicate-valid-unset.input.bin")
                .as_slice(),
            include_bytes!("../parity/preferences/goldens/duplicate-valid-unset.jvm.bin")
                .as_slice(),
        ),
        (
            include_bytes!("../parity/preferences/goldens/valid-repeated-empty-values.input.bin")
                .as_slice(),
            include_bytes!("../parity/preferences/goldens/valid-repeated-empty-values.jvm.bin")
                .as_slice(),
        ),
        (
            include_bytes!("../parity/preferences/goldens/valid-repeated-empty-entries.input.bin")
                .as_slice(),
            include_bytes!("../parity/preferences/goldens/valid-repeated-empty-entries.jvm.bin")
                .as_slice(),
        ),
        (
            include_bytes!("../parity/preferences/goldens/unset-other-key-valid.input.bin")
                .as_slice(),
            include_bytes!("../parity/preferences/goldens/unset-other-key-valid.jvm.bin")
                .as_slice(),
        ),
    ] {
        assert_eq!(Preferences::decode(bytes), Err(Error::MissingValue));
        // JVM parses these, but serializer rejects final VALUE_NOT_SET map values.
        assert_eq!(Preferences::decode(jvm), Err(Error::MissingValue));
    }
}

#[test]
fn unset_physical_entries_keep_entry_and_field_budgets() {
    let unset = include_bytes!("../parity/preferences/goldens/missing-value.input.bin");
    let valid = wrap(&[24, 1]);
    let mut at_limit = unset.repeat(MAX_ENTRIES - 1);
    at_limit.extend_from_slice(&valid);
    assert_eq!(
        Preferences::decode(&at_limit).unwrap().get("x"),
        Some(&Value::Integer(1))
    );
    let mut over_limit = unset.to_vec();
    over_limit.extend_from_slice(&at_limit);
    assert_eq!(Preferences::decode(&over_limit), Err(Error::LimitExceeded));
    let mut entry = vec![10, 1, b'x'];
    for _ in 0..65_535 {
        field(&mut entry, 18, &[]);
    }
    let mut too_many_fields = Vec::new();
    field(&mut too_many_fields, 10, &entry);
    too_many_fields.extend_from_slice(&valid);
    assert_eq!(
        Preferences::decode(&too_many_fields),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn fixture_effective_unset_validation_read_modify_reopen() {
    let directory = Directory::new();
    let valid = include_bytes!("../parity/preferences/goldens/duplicate-unset-valid.input.bin");
    fs::write(directory.data(), valid).unwrap();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(store.preferences().get("x"), Some(&Value::Integer(1)));
    store
        .update(|doc| doc.set("x".to_owned(), Value::Integer(2)))
        .unwrap();
    drop(store);
    assert_eq!(
        FixtureStore::open(&directory.0)
            .unwrap()
            .preferences()
            .get("x"),
        Some(&Value::Integer(2))
    );
    let unset = include_bytes!("../parity/preferences/goldens/duplicate-valid-unset.input.bin");
    fs::write(directory.data(), unset).unwrap();
    assert!(matches!(
        FixtureStore::open(&directory.0),
        Err(Error::MissingValue)
    ));
    assert_eq!(fs::read(directory.data()).unwrap(), unset);
    for unknown in [
        include_bytes!("../parity/preferences/goldens/duplicate-future-valid.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/duplicate-unknown-unset-valid.input.bin")
            .as_slice(),
    ] {
        fs::write(directory.data(), unknown).unwrap();
        let mut store = FixtureStore::open(&directory.0).unwrap();
        assert_eq!(store.preferences().get("x"), Some(&Value::Integer(1)));
        assert_eq!(
            store.update(|_| panic!("read-only callback must not run")),
            Err(Error::UnknownFields)
        );
        drop(store);
        assert_eq!(fs::read(directory.data()).unwrap(), unknown);
        assert!(
            FixtureStore::open(&directory.0)
                .unwrap()
                .preferences()
                .is_read_only()
        );
    }
}

#[test]
fn cached_jvm_unknown_wire_retained_and_all_mutation_refused() {
    for bytes in [
        include_bytes!("../parity/preferences/goldens/unknown-top.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/unknown-value.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/unknown-set.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/unknown-entry.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/unknown-group.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/wrong-known-wire.input.bin").as_slice(),
    ] {
        let mut doc = Preferences::decode(bytes).unwrap();
        assert!(doc.is_read_only());
        assert_eq!(
            doc.set("unrelated".to_owned(), Value::Boolean(true)),
            Err(Error::UnknownFields)
        );
        assert_eq!(doc.remove("absent"), Err(Error::UnknownFields));
        assert_eq!(doc.encode().unwrap(), bytes);
    }
    let original = include_bytes!("../parity/preferences/goldens/unknown-entry.input.bin");
    let jvm = include_bytes!("../parity/preferences/goldens/unknown-entry.jvm.bin");
    assert_ne!(original.as_slice(), jvm.as_slice()); // JVM drops map-entry unknown field.
    let mut duplicate = original.to_vec();
    duplicate.extend_from_slice(&wrap(&[24, 2]));
    let doc = Preferences::decode(&duplicate).unwrap();
    assert_eq!(doc.get("x"), Some(&Value::Integer(2)));
    assert!(doc.is_read_only());
}

#[test]
fn strict_utf8_is_documented_narrower_boundary_and_unset_fails() {
    for bytes in [
        include_bytes!("../parity/preferences/goldens/invalid-utf8.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/invalid-set.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/invalid-key.input.bin").as_slice(),
    ] {
        assert_eq!(Preferences::decode(bytes), Err(Error::InvalidUtf8));
    }
    // JVM goldens prove value/set replacement rather than matching our rejection.
    assert_eq!(
        Preferences::decode(include_bytes!(
            "../parity/preferences/goldens/invalid-utf8.jvm.bin"
        ))
        .unwrap()
        .get("x"),
        Some(&Value::String("�".to_owned()))
    );
    assert_eq!(
        Preferences::decode(include_bytes!(
            "../parity/preferences/goldens/invalid-set.jvm.bin"
        ))
        .unwrap()
        .get("x"),
        Some(&set(&["�"]))
    );
    for bytes in [
        include_bytes!("../parity/preferences/goldens/missing-value.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/future-only.input.bin").as_slice(),
    ] {
        assert_eq!(Preferences::decode(bytes), Err(Error::MissingValue));
    }
}

#[test]
fn new_encoder_matches_jvm_extremes_and_roundtrips_every_variant() {
    let mut doc = Preferences::default();
    for (key, value) in [
        ("a_int", Value::Integer(i32::MAX)),
        ("b_long", Value::Long(i64::MAX)),
        ("c_minus", Value::Integer(-1)),
        ("d_float", Value::FloatBits(0x8000_0000)),
        ("e_double", Value::DoubleBits(0x7ff0_0000_0000_0000)),
    ] {
        doc.set(key.to_owned(), value).unwrap();
    }
    assert_eq!(doc.encode().unwrap(), EXTREMES);
    for value in [
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(i32::MIN),
        Value::Integer(0),
        Value::Long(i64::MIN),
        Value::Long(-1),
        Value::Long(0),
        Value::FloatBits(0),
        Value::FloatBits(0xff80_0000),
        Value::FloatBits(0x7f80_0001),
        Value::DoubleBits(0x8000_0000_0000_0000),
        Value::DoubleBits(0x7ff0_0000_0000_0001),
        Value::String("雪\0😀".to_owned()),
        Value::String(String::new()),
        set(&["", "a", "雪"]),
        set(&[]),
        Value::Bytes(vec![255, 0]),
        Value::Bytes(Vec::new()),
    ] {
        doc.set("x".to_owned(), value.clone()).unwrap();
        assert_eq!(
            Preferences::decode(&doc.encode().unwrap())
                .unwrap()
                .get("x"),
            Some(&value)
        );
    }
    doc.remove("x").unwrap();
    doc.remove("absent").unwrap();
    assert_eq!(doc.get("x"), None);
}

#[test]
fn corruption_truncation_wire_types_and_extreme_lengths() {
    for bytes in [
        include_bytes!("../parity/preferences/goldens/truncated.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/invalid-tag.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/bad-varint.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/unclosed-group.input.bin").as_slice(),
        &[12],
        &[14],
        &[15],
        &[0x53, 0x5c],
        &[0x53, 8, 0],
        &[0xff, 0xff, 0xff, 0xff, 0xff, 0x01],
        &[0x48, 0x80],
        &[
            0x48, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 2,
        ],
        &[
            0x4a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 1,
        ],
        &[0x49],
        &[0x4d],
    ] {
        assert!(Preferences::decode(bytes).is_err());
    }
    for prefix in 0..ALL.len() {
        let bytes = &ALL[..prefix];
        if let Ok(doc) = Preferences::decode(bytes) {
            assert_eq!(doc.encode().unwrap(), bytes);
        }
    }
    for value in [
        &[21, 0, 0, 0][..],
        &[57, 0, 0, 0],
        &[66, 3, 0],
        &[42, 2, b'a'],
        &[50, 2, 10],
    ] {
        assert!(Preferences::decode(&wrap(value)).is_err());
    }
    // Unknown fixed-width, bytes, groups all retain exact original bytes.
    let mut bytes = wrap(&[24, 1]);
    bytes.extend_from_slice(&[
        0x49, 1, 2, 3, 4, 5, 6, 7, 8, 0x4d, 1, 2, 3, 4, 0x4a, 2, 255, 0,
    ]);
    assert_eq!(
        Preferences::decode(&bytes).unwrap().encode().unwrap(),
        bytes
    );
}

#[test]
fn limits_enforced_on_decode_and_mutation_without_partial_change() {
    assert_eq!(
        Preferences::decode(&vec![0; MAX_BYTES + 1]),
        Err(Error::LimitExceeded)
    );
    let mut duplicate = Vec::new();
    for _ in 0..=MAX_ENTRIES {
        duplicate.extend_from_slice(&wrap(&[24, 0]));
    }
    assert_eq!(Preferences::decode(&duplicate), Err(Error::LimitExceeded));
    let mut strings = Vec::new();
    for _ in 0..=MAX_SET_STRINGS {
        field(&mut strings, 10, b"");
    }
    let mut value = Vec::new();
    field(&mut value, 50, &strings);
    assert_eq!(
        Preferences::decode(&wrap(&value)),
        Err(Error::LimitExceeded)
    );
    let fields: Vec<u8> = [0x48, 0].into_iter().cycle().take(2 * 65_537).collect();
    assert_eq!(Preferences::decode(&fields), Err(Error::LimitExceeded));
    let mut groups = vec![0x53; 32];
    groups.extend_from_slice(&[0x54; 32]);
    assert!(Preferences::decode(&groups).unwrap().is_read_only());
    groups.insert(0, 0x53);
    groups.push(0x54);
    assert_eq!(Preferences::decode(&groups), Err(Error::LimitExceeded));
    let mut doc = Preferences::decode(ALL).unwrap();
    let old = doc.clone();
    assert_eq!(
        doc.set("large".to_owned(), Value::Bytes(vec![0; MAX_BYTES])),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        doc.set("x".repeat(MAX_BYTES), Value::Boolean(true)),
        Err(Error::LimitExceeded)
    );
    assert_eq!(
        PreferenceSet::from_strings((0..=MAX_SET_STRINGS).map(|i| i.to_string())),
        Err(Error::LimitExceeded)
    );
    assert_eq!(doc, old);
    let mut exact = Preferences::default();
    exact
        .set(String::new(), Value::Bytes(vec![0; MAX_BYTES - 14]))
        .unwrap();
    let encoded = exact.encode().unwrap();
    assert_eq!(encoded.len(), MAX_BYTES);
    assert_eq!(
        Preferences::decode(&encoded).unwrap().encode().unwrap(),
        encoded
    );
    assert_eq!(
        exact.set(String::new(), Value::Bytes(vec![0; MAX_BYTES - 13])),
        Err(Error::LimitExceeded)
    );
}

#[test]
fn bounded_random_wire_never_panics_and_untouched_success_is_exact() {
    let mut seed = 0x1234_5678_90ab_cdef_u64;
    for len in 0..512 {
        let mut bytes = Vec::new();
        for _ in 0..len {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            bytes.push(seed.to_le_bytes()[0]);
        }
        if let Ok(doc) = Preferences::decode(&bytes) {
            assert_eq!(doc.encode().unwrap(), bytes);
        }
    }
}

#[test]
fn debug_and_errors_redact_keys_values_and_paths() {
    let directory = Directory::new();
    let store = FixtureStore::open(&directory.0).unwrap();
    let value = Value::String("synthetic-secret".to_owned());
    let mut doc = Preferences::default();
    doc.set("synthetic-key".to_owned(), value.clone()).unwrap();
    assert_eq!(format!("{store:?}"), "FixtureStore([redacted])");
    assert_eq!(format!("{doc:?}"), "Preferences([redacted])");
    assert_eq!(format!("{value:?}"), "Value([redacted])");
    assert_eq!(Error::Io.to_string(), "preferences error: Io");
    let error: &dyn std::error::Error = &Error::InvalidWire;
    assert!(error.source().is_none());
}

#[test]
fn fixture_missing_defaults_atomic_modify_reopen_and_ownership() {
    let directory = Directory::new();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    assert!(store.preferences().is_empty());
    assert!(!directory.data().exists());
    assert!(matches!(
        FixtureStore::open(&directory.0),
        Err(Error::Owned)
    ));
    assert_eq!(
        store.update(|doc| doc.set("x".to_owned(), Value::Integer(-1))),
        Ok(CommitOutcome::Synced)
    );
    store
        .update(|doc| {
            assert_eq!(format!("{doc:?}"), "FixtureEdit([redacted])");
            doc.set("y".to_owned(), Value::String("雪".to_owned()))
        })
        .unwrap();
    drop(store);
    let mut reopened = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(reopened.preferences().get("x"), Some(&Value::Integer(-1)));
    assert_eq!(
        reopened.preferences().get("y"),
        Some(&Value::String("雪".to_owned()))
    );
    reopened.update(|doc| doc.remove("x")).unwrap();
    drop(reopened);
    assert_eq!(
        FixtureStore::open(&directory.0)
            .unwrap()
            .preferences()
            .get("x"),
        None
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2); // data + stable lock, no temp.
}

#[test]
fn fixture_corrupt_unknown_oversized_and_external_drift_never_overwrite() {
    let directory = Directory::new();
    let corrupt = [10, 5, 10];
    fs::write(directory.data(), corrupt).unwrap();
    assert!(matches!(
        FixtureStore::open(&directory.0),
        Err(Error::Truncated)
    ));
    assert_eq!(fs::read(directory.data()).unwrap(), corrupt);
    for invalid in [
        include_bytes!("../parity/preferences/goldens/invalid-utf8.input.bin").as_slice(),
        include_bytes!("../parity/preferences/goldens/invalid-set.input.bin").as_slice(),
    ] {
        fs::write(directory.data(), invalid).unwrap();
        assert!(matches!(
            FixtureStore::open(&directory.0),
            Err(Error::InvalidUtf8)
        ));
        assert_eq!(fs::read(directory.data()).unwrap(), invalid);
    }
    let unknown = include_bytes!("../parity/preferences/goldens/unknown-value.input.bin");
    fs::write(directory.data(), unknown).unwrap();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(
        store.update(|_| panic!("unknown callback must not run")),
        Err(Error::UnknownFields)
    );
    assert_eq!(fs::read(directory.data()).unwrap(), unknown);
    drop(store);
    fs::write(directory.data(), ALL).unwrap();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    fs::write(directory.data(), EXTREMES).unwrap();
    assert_eq!(
        store.update(|_| panic!("drift callback must not run")),
        Err(Error::ExternalChange)
    );
    assert_eq!(fs::read(directory.data()).unwrap(), EXTREMES);
    assert_eq!(
        store.preferences().get("b_int"),
        Some(&Value::Integer(i32::MIN))
    );
    drop(store);
    fs::write(directory.data(), vec![0; MAX_BYTES + 1]).unwrap();
    assert!(matches!(
        FixtureStore::open(&directory.0),
        Err(Error::LimitExceeded)
    ));
    fs::remove_file(directory.data()).unwrap();
    fs::create_dir(directory.data()).unwrap();
    assert!(matches!(FixtureStore::open(&directory.0), Err(Error::Io)));
}

#[test]
fn fixture_callback_and_real_temp_create_failure_preserve_reopened_bytes() {
    let directory = Directory::new();
    fs::write(directory.data(), ALL).unwrap();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(
        store.update(|doc| {
            doc.remove("a_bool")?;
            Err(Error::InvalidWire)
        }),
        Err(Error::InvalidWire)
    );
    assert_eq!(store.preferences().encode().unwrap(), ALL);
    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = store.update(|doc| {
            doc.remove("a_bool")?;
            panic!("synthetic callback panic");
        });
    }));
    assert!(panic_result.is_err());
    assert_eq!(store.preferences().encode().unwrap(), ALL);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o500)).unwrap();
        let result = store.update(|doc| doc.remove("a_bool"));
        fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result, Err(Error::Io));
    }
    drop(store);
    assert_eq!(fs::read(directory.data()).unwrap(), ALL);
    assert_eq!(
        FixtureStore::open(&directory.0)
            .unwrap()
            .preferences()
            .encode()
            .unwrap(),
        ALL
    );
}

#[test]
fn fixture_second_drift_check_and_absent_vs_empty_are_distinct() {
    let directory = Directory::new();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    fs::write(directory.data(), []).unwrap();
    assert_eq!(store.update(|_| Ok(())), Err(Error::ExternalChange));
    drop(store);
    fs::write(directory.data(), ALL).unwrap();
    let mut store = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(
        store.update(|doc| {
            doc.remove("a_bool")?;
            fs::write(directory.data(), EXTREMES).unwrap();
            Ok(())
        }),
        Err(Error::ExternalChange)
    );
    assert_eq!(fs::read(directory.data()).unwrap(), EXTREMES);
    assert_eq!(store.preferences().encode().unwrap(), ALL);
    drop(store);
    assert_eq!(
        FixtureStore::open(&directory.0)
            .unwrap()
            .preferences()
            .encode()
            .unwrap(),
        EXTREMES
    );
}

#[test]
fn lock_holder_helper_process() {
    let Some(directory) = std::env::var_os("EH_PREFERENCES_SYNTHETIC_LOCK_HELPER") else {
        return;
    };
    let store = FixtureStore::open(Path::new(&directory)).unwrap();
    fs::write(Path::new(&directory).join("ready"), []).unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::hint::black_box(&store);
    }
}

#[test]
fn real_helper_process_contention_crash_releases_stable_lock_inode() {
    let directory = Directory::new();
    fs::write(directory.data(), ALL).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_holder_helper_process", "--nocapture"])
        .env("EH_PREFERENCES_SYNTHETIC_LOCK_HELPER", &directory.0)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !directory.0.join("ready").exists() {
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("synthetic lock helper timeout");
        }
        assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(matches!(
        FixtureStore::open(&directory.0),
        Err(Error::Owned)
    ));
    let before = fs::metadata(directory.0.join(".preferences.lock")).unwrap();
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    let mut reopened = FixtureStore::open(&directory.0).unwrap();
    assert_eq!(reopened.preferences().encode().unwrap(), ALL);
    reopened
        .update(|doc| doc.set("after-crash".to_owned(), Value::Boolean(true)))
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        assert_eq!(
            before.ino(),
            fs::metadata(directory.0.join(".preferences.lock"))
                .unwrap()
                .ino()
        );
    }
}

#[test]
fn ordered_preference_set_constructor_insertion_redaction_and_bounds() {
    let mut ordered = PreferenceSet::from_strings(["雪", "x", "雪"].map(str::to_owned)).unwrap();
    assert_eq!(ordered.iter().collect::<Vec<_>>(), ["雪", "x"]);
    assert_eq!(ordered.len(), 2);
    assert!(!ordered.is_empty());
    assert_eq!(ordered.insert("雪".to_owned()), Ok(false));
    assert_eq!(ordered.insert("😀".to_owned()), Ok(true));
    assert_eq!(ordered.iter().collect::<Vec<_>>(), ["雪", "x", "😀"]);
    assert_eq!(format!("{ordered:?}"), "PreferenceSet([redacted])");
    let empty = PreferenceSet::default();
    assert!(empty.is_empty());
    assert_eq!(empty.iter().len(), 0);
    assert_eq!(
        PreferenceSet::from_strings(std::iter::repeat_n(String::new(), MAX_SET_STRINGS + 1)),
        Err(Error::LimitExceeded)
    );
    let mut full =
        PreferenceSet::from_strings((0..MAX_SET_STRINGS).map(|i| i.to_string())).unwrap();
    let original = full.clone();
    assert_eq!(full.insert("0".to_owned()), Ok(false));
    assert_eq!(full.insert("new".to_owned()), Err(Error::LimitExceeded));
    assert_eq!(full, original);
    let mut byte_limit = PreferenceSet::default();
    assert_eq!(
        byte_limit.insert("x".repeat(MAX_BYTES)),
        Err(Error::LimitExceeded)
    );
    assert!(byte_limit.is_empty());
    byte_limit.insert("x".repeat(MAX_BYTES - 4)).unwrap();
    let original = byte_limit.clone();
    assert_eq!(byte_limit.insert(String::new()), Err(Error::LimitExceeded));
    assert_eq!(byte_limit, original);
    let mut doc = Preferences::decode(ALL).unwrap();
    let original = doc.clone();
    assert_eq!(
        doc.set("large".to_owned(), Value::StringSet(byte_limit)),
        Err(Error::LimitExceeded)
    );
    assert_eq!(doc, original);
    // Mutation/read/encode preserves source-exact attempted-password order.
    let Value::StringSet(mut passwords) = doc.get("f_set").unwrap().clone() else {
        panic!("set fixture expected");
    };
    assert_eq!(passwords.iter().collect::<Vec<_>>(), ["雪", "x"]);
    assert_eq!(passwords.insert("雪".to_owned()), Ok(false));
    passwords.insert("new".to_owned()).unwrap();
    doc.set("f_set".to_owned(), Value::StringSet(passwords))
        .unwrap();
    let reopened = Preferences::decode(&doc.encode().unwrap()).unwrap();
    let Some(Value::StringSet(passwords)) = reopened.get("f_set") else {
        panic!("set fixture expected");
    };
    assert_eq!(passwords.iter().collect::<Vec<_>>(), ["雪", "x", "new"]);
}
