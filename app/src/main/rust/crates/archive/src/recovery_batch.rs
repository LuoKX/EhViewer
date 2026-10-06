//! Versioned, bounded recovery inspection plans over already indexed image entries.
//!
//! Codec adapters own decoding and clocks. This module owns range validation,
//! cumulative readability, completion and stable recovery issues, without handles.

use std::ops::Range;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable recovery batch protocol version.
pub const RECOVERY_BATCH_VERSION: u8 = 3;
/// Maximum entries inspected during one native call.
pub const MAX_RECOVERY_BATCH_ENTRIES: u32 = 32;
/// Maximum UTF-8 entry-name bytes accepted by this bounded wire boundary.
pub const MAX_RECOVERY_BATCH_NAME_BYTES: usize = 65_536;

/// Sanitized request; cumulative readability comes from the preceding Rust result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBatchRequest {
    /// Negotiated protocol version.
    pub version: u8,
    /// First image not inspected by preceding batches.
    pub start_index: u32,
    /// Maximum number of entries in this batch.
    pub max_entries: u32,
    /// Android's limit for the complete archive's image count.
    pub maximum_entries: u32,
    /// Whether every image in preceding batches was readable.
    pub all_readable: bool,
}

/// Codec observations, never supplied by an untrusted JNI request.
#[derive(Clone, Copy, Debug)]
pub struct RecoveryBatchContext {
    /// Total indexed playable images; zero for a locked archive.
    pub entry_count: u32,
    /// Whether the reader requires a password.
    pub encrypted: bool,
    /// Whether indexing found any unsafe archive pathname.
    pub unsafe_path: bool,
}

/// One indexed image and its fully decoded integrity result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBatchEntry {
    /// Archive-relative image name, not included in diagnostics.
    pub name: String,
    /// Declared uncompressed image bytes.
    pub size: u64,
    /// Whether full decoding and integrity verification succeeded.
    pub readable: bool,
}

/// Closed issue set shared with the existing download recovery contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryBatchIssue {
    /// The archive needs a password and recovery never supplies one.
    EncryptedArchive,
    /// The archive exceeds the caller's image-count limit.
    LimitExceeded,
    /// The index contains an absolute or parent-traversing pathname.
    UnsafeArchivePath,
}

/// One bounded batch and the authoritative cumulative inspection state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryBatch {
    /// Total image count from the codec index.
    pub entry_count: u32,
    /// First image to request next; equals count at normal completion.
    pub next_index: u32,
    /// Readability of all images inspected in this and preceding batches.
    pub all_readable: bool,
    /// Every indexed image was inspected and readable.
    pub complete: bool,
    /// Stable archive-wide issues; unsafe paths are published at the final batch.
    pub issues: Vec<RecoveryBatchIssue>,
    /// Ordered facts for this batch only.
    pub entries: Vec<RecoveryBatchEntry>,
}

/// Fixed, input-free request or codec-observation error.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecoveryBatchError {
    /// Invalid version, bounds, initial state or codec count.
    #[error("archive recovery batch request is invalid")]
    InvalidRequest,
    /// A codec supplied a non-progressing, oversized or invalid batch.
    #[error("archive recovery batch observations are invalid")]
    InvalidEntries,
}

/// Plans the largest allowed range before the codec performs any decoding.
///
/// # Errors
/// Rejects invalid versions, zero/excessive batch sizes, zero limits, invalid
/// initial readability, out-of-range cursors and empty unlocked archives.
pub fn plan_recovery_batch(
    context: RecoveryBatchContext,
    request: &RecoveryBatchRequest,
) -> Result<Range<u32>, RecoveryBatchError> {
    if request.version != RECOVERY_BATCH_VERSION
        || !(1..=MAX_RECOVERY_BATCH_ENTRIES).contains(&request.max_entries)
        || request.maximum_entries == 0
        || request.start_index > context.entry_count
        || (request.start_index == 0 && !request.all_readable)
        || (context.entry_count == 0 && !context.encrypted)
        || (context.encrypted && (context.entry_count != 0 || request.start_index != 0))
    {
        return Err(RecoveryBatchError::InvalidRequest);
    }
    let count = if context.encrypted || context.entry_count > request.maximum_entries {
        0
    } else {
        request
            .max_entries
            .min(context.entry_count - request.start_index)
    };
    Ok(request.start_index..request.start_index + count)
}

/// Summarizes a possibly shortened batch after the codec's cooperative time slice.
///
/// # Errors
/// Rejects invalid requests or invalid image facts. A nonterminal, eligible batch
/// must contain at least one entry so callers cannot enter a non-progressing loop.
pub fn finish_recovery_batch(
    context: RecoveryBatchContext,
    request: &RecoveryBatchRequest,
    entries: Vec<RecoveryBatchEntry>,
) -> Result<RecoveryBatch, RecoveryBatchError> {
    let range = plan_recovery_batch(context, request)?;
    // Saturation remains above the 32-entry range and is rejected below.
    let inspected = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    if inspected > range.end - range.start
        || (!range.is_empty() && entries.is_empty())
        || entries.iter().any(|entry| {
            entry.name.is_empty()
                || entry.name.len() > MAX_RECOVERY_BATCH_NAME_BYTES
                || !(1..=512 * 1024 * 1024).contains(&entry.size)
        })
    {
        return Err(RecoveryBatchError::InvalidEntries);
    }
    let next_index = request.start_index + inspected;
    let all_readable = request.all_readable && entries.iter().all(|entry| entry.readable);
    let mut issues = Vec::new();
    if context.encrypted {
        issues.push(RecoveryBatchIssue::EncryptedArchive);
    } else if context.entry_count > request.maximum_entries {
        issues.push(RecoveryBatchIssue::LimitExceeded);
    } else if next_index == context.entry_count && context.unsafe_path {
        issues.push(RecoveryBatchIssue::UnsafeArchivePath);
    }
    Ok(RecoveryBatch {
        entry_count: context.entry_count,
        next_index,
        all_readable,
        complete: !context.encrypted
            && context.entry_count <= request.maximum_entries
            && next_index == context.entry_count
            && all_readable,
        issues,
        entries,
    })
}

#[cfg(test)]
mod tests;
