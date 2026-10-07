//! Focused source-backed schema contracts; no persistent engine or device claims.
#![deny(missing_docs, unsafe_code, rust_2018_idioms, unused_lifetimes)]
#![deny(clippy::all, clippy::pedantic)]

#[path = "../src/database_schema.rs"]
mod database_schema;

use database_schema::{Column, Database, Entity, Error, Schema, schema, schemas};

const EH_IDENTITIES: &[&str] = &[
    "f5752deda842dccabded1cdbf1c78384", // Original 4 and 5 share identity, not version.
    "f5752deda842dccabded1cdbf1c78384",
    "e45f4380ef769c127604cb2a502a942b",
    "9563c2017be4341dc1bada115426df77",
    "f09bae277bbdab54ecbb0d698c83e734",
    "64f51a531e13d2b0b971f3d8da0876b0",
    "613604f93bf815adeb87f03e2166e983",
    "92915e937cb7ccc536d352fda5cf287f",
    "d40b38b0ba3b1c53b773728cdee328c8",
    "df15d8f70f45047a738c8911212ea73c",
    "54e4f2704cc47c5a6a2f4b96de978cce",
    "991a3a3ddc4fe8d4dbcb39b975ab73b8",
    "06e5ff8fd6797d5801e86753fa5831b2",
    "d6375317a9298b7feae5925a23272bb9",
    "bc90acf4ffb4e7cce9a16cd5a0f662f3",
    "edd2f8e628072889f7702b50eb2c44a1",
    "a8dd0226b286cc57a8c99ba7ad416730",
    "6a9926f6722f8fc6f9c4426b31260e33",
    "f61ae3beef3739c0de7db945d2c9a4a4",
    "7d1cfc4e9a9d506a73eb0520ae414d25",
    "0d05b032b9d5fbd1358d6e5803832535",
];

fn entity<'a>(value: &'a Schema, table: &str) -> &'a Entity {
    value
        .entities
        .iter()
        .find(|entity| entity.table == table)
        .unwrap()
}

fn column<'a>(value: &'a Entity, name: &str) -> &'a Column {
    value
        .columns
        .iter()
        .find(|column| column.name == name)
        .unwrap()
}

#[test]
fn exact_original_versions_identities_and_no_nearest_fallback() {
    let eh = schemas(Database::Eh);
    assert_eq!(eh.len(), 21);
    for (index, version) in (4..=24).enumerate() {
        let exact = schema(Database::Eh, version).unwrap();
        assert!(std::ptr::eq(exact, std::ptr::from_ref(&eh[index])));
        assert_eq!(exact.database, Database::Eh);
        assert_eq!(exact.version, version);
        assert_eq!(exact.identity_hash, EH_IDENTITIES[index]);
        assert_eq!(exact.source_sha256.len(), 64);
        assert!(
            exact
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
    }
    let search = schemas(Database::Search);
    assert_eq!(search.len(), 2);
    for (index, version) in (1..=2).enumerate() {
        let exact = schema(Database::Search, version).unwrap();
        assert!(std::ptr::eq(exact, std::ptr::from_ref(&search[index])));
        assert_eq!(exact.database, Database::Search);
        assert_eq!(exact.version, version);
        assert_eq!(
            exact.identity_hash,
            [
                "69230710633494b4b331e10dae436fb7",
                "bf4a5b5f3cb9bc7383488bae14f5d4bd"
            ][index]
        );
    }
    for version in [0, 1, 2, 3, 25, u32::MAX] {
        assert_eq!(
            schema(Database::Eh, version).unwrap_err(),
            Error::UnsupportedVersion
        );
    }
    for version in [0, 3, 24, u32::MAX] {
        assert_eq!(
            schema(Database::Search, version).unwrap_err(),
            Error::UnsupportedVersion
        );
    }
}

#[test]
fn all_original_ddl_only_substitutes_table_name_and_keeps_order() {
    for database in [Database::Eh, Database::Search] {
        for value in schemas(database) {
            let mut expected = Vec::new();
            for entity in value.entities {
                assert!(entity.create_sql.contains("`${TABLE_NAME}`"));
                expected.push(entity.create_sql.replace("${TABLE_NAME}", entity.table));
                for field in entity.columns {
                    assert!(!field.field_path.is_empty());
                    assert!(!field.name.is_empty());
                    assert!(!field.affinity.is_empty());
                }
            }
            for entity in value.entities {
                for index in entity.indices {
                    assert!(!index.name.is_empty());
                    assert!(!index.columns.is_empty());
                    assert!(index.orders.is_empty() || index.orders.len() == index.columns.len());
                    expected.push(index.create_sql.replace("${TABLE_NAME}", entity.table));
                }
            }
            for entity in value.entities {
                for trigger in entity.content_sync_triggers {
                    expected.push(trigger.replace("${TABLE_NAME}", entity.table));
                }
                for fk in entity.foreign_keys {
                    assert!(!fk.table.is_empty());
                    assert_eq!(fk.columns.len(), fk.referenced_columns.len());
                }
            }
            expected.extend(value.setup_queries.iter().map(ToString::to_string));
            assert_eq!(expected, value.ddl);
            assert_eq!(value.setup_queries.len(), 2);
            assert_eq!(
                value.setup_queries[0],
                "CREATE TABLE IF NOT EXISTS room_master_table (id INTEGER PRIMARY KEY,identity_hash TEXT)"
            );
            assert_eq!(
                value.setup_queries[1],
                format!(
                    "INSERT OR REPLACE INTO room_master_table (id,identity_hash) VALUES(42, '{}')",
                    value.identity_hash
                )
            );
            assert!(
                !value
                    .ddl
                    .iter()
                    .any(|statement| statement.contains("${TABLE_NAME}"))
            );
            assert!(
                !value
                    .ddl
                    .iter()
                    .any(|statement| statement.contains("user_version"))
            );
        }
    }
}

#[test]
fn original_search_long_and_nullability_are_not_normalized() {
    let first = schema(Database::Search, 1).unwrap();
    let second = schema(Database::Search, 2).unwrap();
    assert_eq!(
        column(entity(first, "suggestions"), "date").affinity,
        "LONG"
    );
    assert!(!column(entity(first, "suggestions"), "date").not_null);
    assert_eq!(
        column(entity(second, "suggestions"), "date").affinity,
        "INTEGER"
    );
    assert!(column(entity(second, "suggestions"), "date").not_null);
    assert!(
        !column(
            entity(schema(Database::Eh, 17).unwrap(), "GALLERIES"),
            "TOKEN"
        )
        .not_null
    );
    assert!(
        column(
            entity(schema(Database::Eh, 18).unwrap(), "GALLERIES"),
            "TOKEN"
        )
        .not_null
    );
    let current = entity(schema(Database::Eh, 24).unwrap(), "GALLERIES");
    assert_eq!(column(current, "PAGES").default_value, Some("0"));
    assert_eq!(column(current, "TITLE").default_value, None);
    assert!(!column(current, "TITLE").not_null);
}

#[test]
fn original_current_keys_indices_actions_and_fts_triggers() {
    let current = schema(Database::Eh, 24).unwrap();
    let labels = entity(current, "DOWNLOAD_LABELS");
    assert_eq!(labels.primary_key.columns, &["_id"]);
    assert!(!labels.primary_key.auto_generate);
    assert!(!column(labels, "_id").not_null);
    assert!(labels.indices[0].unique);
    assert_eq!(labels.indices[0].columns, &["LABEL"]);
    let downloads = entity(current, "DOWNLOADS");
    assert_eq!(downloads.foreign_keys[0].table, "GALLERIES");
    assert_eq!(downloads.foreign_keys[0].on_delete, "NO ACTION");
    assert_eq!(downloads.foreign_keys[1].table, "DOWNLOAD_LABELS");
    assert_eq!(downloads.foreign_keys[1].on_delete, "SET NULL");
    assert_eq!(downloads.foreign_keys[1].on_update, "CASCADE");
    assert_eq!(
        entity(current, "PROGRESS").foreign_keys[0].on_delete,
        "CASCADE"
    );
    let artists = entity(current, "DOWNLOAD_ARTISTS");
    assert_eq!(artists.primary_key.columns, &["GID", "ARTIST"]);
    assert_eq!(artists.foreign_keys[0].on_delete, "CASCADE");
    let recovery = entity(current, "DOWNLOAD_RECOVERY_CANDIDATES");
    assert!(recovery.primary_key.auto_generate);
    assert_eq!(recovery.indices.len(), 4);
    assert!(recovery.indices[0].unique);
    assert!(
        recovery
            .create_sql
            .contains("INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL")
    );
    for version in 21..=24 {
        let value = schema(Database::Eh, version).unwrap();
        let fts = entity(
            value,
            if version == 21 {
                "GalleryFts"
            } else {
                "GALLERIES_FTS"
            },
        );
        assert_eq!(fts.fts_version, Some("FTS4"));
        assert!(
            fts.fts_options_json
                .unwrap()
                .contains("\"contentTable\":\"GALLERIES\"")
        );
        assert!(
            !fts.fts_options_json
                .unwrap()
                .contains("contentSyncTriggers")
        );
        assert_eq!(fts.content_sync_triggers.len(), 4);
        for trigger in fts.content_sync_triggers {
            assert!(trigger.starts_with("CREATE TRIGGER IF NOT EXISTS room_fts_content_sync_"));
            assert!(value.ddl.contains(trigger));
        }
    }
    assert!(
        schema(Database::Eh, 20)
            .unwrap()
            .entities
            .iter()
            .all(|entity| entity.fts_version.is_none())
    );
}

#[test]
fn descriptor_debug_and_errors_are_redacted_even_for_constructed_input() {
    let value = schema(Database::Eh, 24).unwrap();
    let labels = entity(value, "DOWNLOAD_LABELS");
    let downloads = entity(value, "DOWNLOADS");
    let untrusted = Column {
        field_path: "private-input",
        name: "private-input",
        affinity: "private-input",
        not_null: false,
        default_value: Some("private-input"),
    };
    assert_eq!(format!("{untrusted:?}"), "Column([redacted])");
    assert_eq!(format!("{value:?}"), "Schema([redacted])");
    assert_eq!(format!("{labels:?}"), "Entity([redacted])");
    assert_eq!(
        format!("{:?}", labels.primary_key),
        "PrimaryKey([redacted])"
    );
    assert_eq!(
        format!("{:?}", downloads.foreign_keys[0]),
        "ForeignKey([redacted])"
    );
    assert_eq!(format!("{:?}", labels.indices[0]), "Index([redacted])");
    assert_eq!(format!("{:?}", Database::Eh), "Eh");
    assert_eq!(format!("{:?}", Database::Search), "Search");
    assert_eq!(
        format!("{}", Error::UnsupportedVersion),
        "schema error: unsupported version"
    );
    assert_eq!(
        format!("{:?}", Error::UnsupportedVersion),
        "UnsupportedVersion"
    );
    assert!(std::error::Error::source(&Error::UnsupportedVersion).is_none());
}
