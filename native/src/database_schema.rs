//! Source-bound Room schema contracts, not a database engine or migration runner.
//!
//! Only retained `EhDatabase` versions 4–24 and `SearchDatabase` versions 1–2 exist.
//! Lookup never discovers paths, opens files, executes SQL, or accepts user SQL.
//! Entity SQL retains original whitespace and quoting; executable DDL substitutes
//! only `${TABLE_NAME}`. Host fixtures prove fresh-schema `SQLite` behavior, not
//! historical Room migration, Android execution, or production compatibility.

use std::fmt;

#[path = "database_schema_data.rs"]
mod data;

/// Independent original Room database namespaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Database {
    /// Gallery/download database; retained schemas start at version 4.
    Eh,
    /// Independent search-suggestion database.
    Search,
}

/// Fail-closed lookup failure; never includes caller input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// No exact retained schema; no nearest-version or destructive fallback.
    UnsupportedVersion,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("schema error: unsupported version")
    }
}
impl std::error::Error for Error {}

/// Original Room column declaration, including persisted SQL nullability.
pub struct Column {
    /// Original entity field path.
    pub field_path: &'static str,
    /// Exact persisted column name.
    pub name: &'static str,
    /// Original exported type label; version 1 search includes `LONG`.
    pub affinity: &'static str,
    /// Original declaration; absent JSON flag means false.
    pub not_null: bool,
    /// Original default SQL expression; absence is distinct from SQL NULL.
    pub default_value: Option<&'static str>,
}

/// Original primary-key declaration; ordering matters for composite keys.
pub struct PrimaryKey {
    /// Room's original generated-key flag, not an inferred rowid policy.
    pub auto_generate: bool,
    /// Ordered original columns.
    pub columns: &'static [&'static str],
}

/// Original foreign-key declaration; no additional constraints invented.
pub struct ForeignKey {
    /// Referenced table name.
    pub table: &'static str,
    /// Exact original ON DELETE action.
    pub on_delete: &'static str,
    /// Exact original ON UPDATE action.
    pub on_update: &'static str,
    /// Ordered local columns.
    pub columns: &'static [&'static str],
    /// Ordered referenced columns.
    pub referenced_columns: &'static [&'static str],
}

/// Explicit original index declaration, excluding `SQLite`'s implicit indices.
pub struct Index {
    /// Original index name.
    pub name: &'static str,
    /// Original uniqueness flag.
    pub unique: bool,
    /// Ordered original column names.
    pub columns: &'static [&'static str],
    /// Original orders; empty means `SQLite`'s default ascending order.
    pub orders: &'static [&'static str],
    /// Untouched original SQL with original table placeholder.
    pub create_sql: &'static str,
}

/// One original Room entity, including entity-level FTS sync triggers.
pub struct Entity {
    /// Exact original table name.
    pub table: &'static str,
    /// Untouched original SQL with original table placeholder.
    pub create_sql: &'static str,
    /// Original column order and declarations.
    pub columns: &'static [Column],
    /// Original primary-key descriptor.
    pub primary_key: PrimaryKey,
    /// Explicit original foreign keys in source order.
    pub foreign_keys: &'static [ForeignKey],
    /// Explicit original indices in source order.
    pub indices: &'static [Index],
    /// Original FTS version, absent for ordinary tables.
    pub fts_version: Option<&'static str>,
    /// Canonical JSON representation of original FTS options, when present.
    pub fts_options_json: Option<&'static str>,
    /// Original entity-level contentSyncTriggers, never read from ftsOptions.
    pub content_sync_triggers: &'static [&'static str],
}

/// Fixed source-bound fresh-schema contract; not a persistent adapter.
pub struct Schema {
    /// Independent original database namespace.
    pub database: Database,
    /// Exact retained Room version.
    pub version: u32,
    /// Exact original Room identity hash.
    pub identity_hash: &'static str,
    /// SHA-256 of original committed schema JSON bytes.
    pub source_sha256: &'static str,
    /// Entity declarations in original order.
    pub entities: &'static [Entity],
    /// Original Room setup queries, unchanged.
    pub setup_queries: &'static [&'static str],
    /// Fixed executable statements: tables, indices, triggers, then setup.
    /// Only original `${TABLE_NAME}` substitution is performed; `user_version`
    /// remains a separate explicit version contract, not invented source SQL.
    pub ddl: &'static [&'static str],
}

// Descriptors may be inspected explicitly, but diagnostics must not serialize
// SQL, names, or caller-created descriptor contents.
macro_rules! redacted_debug {
    ($($name:ident),+ $(,)?) => {$(
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "([redacted])"))
            }
        }
    )+};
}
redacted_debug!(Column, PrimaryKey, ForeignKey, Index, Entity, Schema);

/// Retained versions in ascending order; does not imply migration support.
#[must_use]
pub fn schemas(database: Database) -> &'static [Schema] {
    match database {
        Database::Eh => data::EH_SCHEMAS,
        Database::Search => data::SEARCH_SCHEMAS,
    }
}

/// Look up one exact original version; missing and future versions reject.
///
/// # Errors
/// Returns `UnsupportedVersion` when no source-backed exact version exists.
pub fn schema(database: Database, version: u32) -> Result<&'static Schema, Error> {
    schemas(database)
        .iter()
        .find(|schema| schema.version == version)
        .ok_or(Error::UnsupportedVersion)
}
