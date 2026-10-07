//! Std-only compiled descriptor export for independent host `SQLite` replay.
#![deny(missing_docs, unsafe_code, rust_2018_idioms, unused_lifetimes)]
#![deny(clippy::all, clippy::pedantic)]

#[path = "../../src/database_schema.rs"]
mod database_schema;

use database_schema::{Column, Database, Entity, ForeignKey, Index, Schema};
use std::fmt::Write as _;
use std::io::Write as _;

fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for character in value.chars() {
        match character {
            '\"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            control if control <= '\u{1f}' => {
                write!(result, "\\u{:04x}", u32::from(control)).expect("string formatting");
            }
            other => result.push(other),
        }
    }
    result.push('"');
    result
}

fn optional(value: Option<&str>) -> String {
    value.map_or_else(|| "null".into(), quoted)
}

fn array<T>(values: &[T], item: impl Fn(&T) -> String) -> String {
    format!(
        "[{}]",
        values.iter().map(item).collect::<Vec<_>>().join(",")
    )
}

fn strings(values: &[&str]) -> String {
    array(values, |value| quoted(value))
}

fn column(value: &Column) -> String {
    format!(
        "{{\"fieldPath\":{},\"columnName\":{},\"affinity\":{},\"notNull\":{},\"defaultValue\":{}}}",
        quoted(value.field_path),
        quoted(value.name),
        quoted(value.affinity),
        value.not_null,
        optional(value.default_value)
    )
}

fn foreign_key(value: &ForeignKey) -> String {
    format!(
        "{{\"table\":{},\"onDelete\":{},\"onUpdate\":{},\"columns\":{},\"referencedColumns\":{}}}",
        quoted(value.table),
        quoted(value.on_delete),
        quoted(value.on_update),
        strings(value.columns),
        strings(value.referenced_columns)
    )
}

fn index(value: &Index) -> String {
    format!(
        "{{\"name\":{},\"unique\":{},\"columnNames\":{},\"orders\":{},\"createSql\":{}}}",
        quoted(value.name),
        value.unique,
        strings(value.columns),
        strings(value.orders),
        quoted(value.create_sql)
    )
}

fn entity(value: &Entity) -> String {
    format!(
        concat!(
            "{{\"tableName\":{},\"createSql\":{},\"fields\":{},",
            "\"primaryKey\":{{\"autoGenerate\":{},\"columnNames\":{}}},",
            "\"foreignKeys\":{},\"indices\":{},\"ftsVersion\":{},",
            "\"ftsOptions\":{},\"contentSyncTriggers\":{}}}"
        ),
        quoted(value.table),
        quoted(value.create_sql),
        array(value.columns, column),
        value.primary_key.auto_generate,
        strings(value.primary_key.columns),
        array(value.foreign_keys, foreign_key),
        array(value.indices, index),
        optional(value.fts_version),
        value.fts_options_json.unwrap_or("null"),
        strings(value.content_sync_triggers)
    )
}

fn schema(value: &Schema) -> String {
    let name = match value.database {
        Database::Eh => "EhDatabase",
        Database::Search => "SearchDatabase",
    };
    format!(
        concat!(
            "{{\"name\":{},\"version\":{},\"identity\":{},\"source_sha256\":{},\"ddl\":{},",
            "\"database\":{{\"version\":{},\"identityHash\":{},\"entities\":{},\"setupQueries\":{}}}}}"
        ),
        quoted(name),
        value.version,
        quoted(value.identity_hash),
        quoted(value.source_sha256),
        strings(value.ddl),
        value.version,
        quoted(value.identity_hash),
        array(value.entities, entity),
        strings(value.setup_queries)
    )
}

fn main() {
    // Exact lookup is exercised as well as iteration. No caller inputs accepted.
    let all = [Database::Eh, Database::Search]
        .into_iter()
        .flat_map(database_schema::schemas)
        .map(|value| {
            let exact = database_schema::schema(value.database, value.version)
                .expect("retained schema lookup");
            schema(exact)
        })
        .collect::<Vec<_>>()
        .join(",");
    if std::io::stdout()
        .lock()
        .write_all(format!("[{all}]\n").as_bytes())
        .is_err()
    {
        eprintln!("Schema export failed");
        std::process::exit(1);
    }
}
