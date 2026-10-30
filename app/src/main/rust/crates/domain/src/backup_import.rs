//! Bounded backup download and repaired label merge planning; Room stays Android-owned.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::contracts::{ApiEnvelope, RustError};

/// Stable V1 envelope module.
pub const MODULE: &str = "backupImport";
/// Total snapshot row budget across all request collections.
pub const MAX_ITEMS: usize = 100_000;
/// Per-label raw UTF-16LE byte budget, preserving Java equality including lone surrogates.
pub const MAX_LABEL_BYTES: usize = 8_192;
/// Aggregate raw label byte budget.
pub const MAX_LABEL_TOTAL_BYTES: usize = 4 * 1024 * 1024;
/// Maximum encoded request accepted by the JNI boundary.
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
/// Maximum encoded response (only bounded indices and signed states/positions).
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Only portable facts from one backup download.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportDownload {
    /// Opaque signed gallery identifier; no positivity constraint.
    pub gid: i64,
    /// Arbitrary signed Android state, retaining unknown nonvolatile values.
    pub state: i32,
}

/// Label snapshot; imported positions are intentionally ignored.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportLabel {
    /// Unmodified Java String code units, without Unicode normalization or case folding.
    pub label_utf16: ByteBuf,
    /// Signed persisted position.
    pub position: i32,
}

/// One complete download and/or label import observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupImportRequest {
    /// Must equal one.
    pub version: u8,
    /// Existing identifiers excluded from download additions.
    pub existing_gids: Vec<i64>,
    /// Source-ordered backup downloads.
    pub downloads: Vec<ImportDownload>,
    /// Existing label names and positions, never rewritten.
    pub current_labels: Vec<ImportLabel>,
    /// Source-ordered backup labels; old IDs remain entirely Android-owned.
    pub labels: Vec<ImportLabel>,
}

/// One surviving download bound to its original snapshot slot and identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDownloadAddition {
    /// First surviving occurrence's source slot.
    pub source_index: usize,
    /// Echoed identifier for snapshot validation.
    pub gid: i64,
    /// NONE for WAIT/DOWNLOAD; every other state unchanged.
    pub state: i32,
}

/// One new label; Android copies source row and clears ID before insertion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportLabelAddition {
    /// First new name's source slot.
    pub source_index: usize,
    /// Checked append position.
    pub position: i32,
}

/// Complete validated import plan; an overflow returns no partial payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupImportPlan {
    /// Source-ordered surviving downloads.
    pub downloads: Vec<ImportDownloadAddition>,
    /// Source-ordered new names with contiguous positions.
    pub labels: Vec<ImportLabelAddition>,
}

/// Builds one atomic plan without mutating backup or current rows.
#[must_use]
#[allow(
    clippy::missing_panics_doc,
    reason = "bounded item counts and full position preflight guarantee conversions"
)]
pub fn build_envelope(request: &BackupImportRequest) -> ApiEnvelope<BackupImportPlan> {
    let count = request.existing_gids.len()
        + request.downloads.len()
        + request.current_labels.len()
        + request.labels.len();
    let labels = request.current_labels.iter().chain(&request.labels);
    if request.version != 1
        || count > MAX_ITEMS
        || labels.clone().any(|label| {
            label.label_utf16.len() > MAX_LABEL_BYTES || label.label_utf16.len() % 2 != 0
        })
        || labels.map(|label| label.label_utf16.len()).sum::<usize>() > MAX_LABEL_TOTAL_BYTES
    {
        return failure("Invalid backup import input");
    }
    let mut gids: HashSet<i64> = request.existing_gids.iter().copied().collect();
    let downloads = request
        .downloads
        .iter()
        .enumerate()
        .filter_map(|(source_index, row)| {
            gids.insert(row.gid).then_some(ImportDownloadAddition {
                source_index,
                gid: row.gid,
                state: crate::download_startup::normalize_state(row.state),
            })
        })
        .collect();
    let mut names: HashSet<&[u8]> = request
        .current_labels
        .iter()
        .map(|label| label.label_utf16.as_ref())
        .collect();
    let additions: Vec<usize> = request
        .labels
        .iter()
        .enumerate()
        .filter_map(|(index, row)| names.insert(row.label_utf16.as_ref()).then_some(index))
        .collect();
    let first = (i64::from(
        request
            .current_labels
            .iter()
            .map(|label| label.position)
            .max()
            .unwrap_or(-1),
    ) + 1)
        .max(0);
    // Empty imports remain successful even when existing maximum is Int.MAX_VALUE.
    if first + i64::try_from(additions.len()).expect("bounded label count")
        > i64::from(i32::MAX) + 1
    {
        return failure("Download label positions exceed Int range");
    }
    let labels = additions
        .into_iter()
        .enumerate()
        .map(|(offset, source_index)| {
            ImportLabelAddition {
                source_index,
                // Count/position preflight above guarantees both conversions.
                position: i32::try_from(first + i64::try_from(offset).expect("bounded offset"))
                    .expect("preflight position"),
            }
        })
        .collect();
    ApiEnvelope::ok(1, MODULE, BackupImportPlan { downloads, labels })
}

fn failure(message: &str) -> ApiEnvelope<BackupImportPlan> {
    ApiEnvelope::err(1, MODULE, RustError::Error(message.into()))
}

#[cfg(test)]
mod tests;
