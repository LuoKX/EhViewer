//! Recovery file classification, UTF-16 ordering and stable retention plans.
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::contracts::{ApiEnvelope, RustError};

/// Stable CBOR module identifier.
pub const MODULE: &str = "storageRules";
/// Maximum sortable facts per request.
pub const MAX_ITEMS: usize = 100_000;
/// Maximum classification chunk, keeping JNI working sets small.
pub const MAX_CLASSIFY_ITEMS: usize = 1024;
/// Aggregate raw UTF-16 observation ceiling.
pub const MAX_NAME_BYTES: usize = 16 * 1024 * 1024;

/// Closed storage operation set.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageOperation {
    /// Select directory/archive candidates from platform observations.
    Classify,
    /// Sort selected candidates and apply the candidate count limit.
    Candidates,
    /// Sort physical child names using Java String UTF-16 ordering.
    Names,
    /// Keep newest files and return stable deletion indices.
    Cleanup,
}

/// Closed recovery candidate type; extensions do not imply content validity.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryFileKind {
    /// Non-root directory with an Android-compatible strict page-name observation.
    Directory,
    /// Comic ZIP wrapper.
    Cbz,
    /// Generic ZIP.
    Zip,
    /// RAR.
    Rar,
    /// 7-Zip.
    SevenZip,
    /// TAR.
    Tar,
    /// Gzip TAR.
    TarGz,
    /// XZ TAR.
    TarXz,
}

/// Bounded platform file facts; no paths are interpreted or opened.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileObservation {
    /// Slot in the complete Android node snapshot.
    pub index: u32,
    /// Whether the platform observed a directory.
    pub directory: bool,
    /// Whether this is the scan root.
    pub root: bool,
    /// Device Regex observation, retaining platform Unicode case-folding behavior.
    pub has_strict_page: bool,
    /// Last at most seven UTF-16 code units of a filename, in little-endian bytes.
    pub suffix_utf16: ByteBuf,
}

/// Selected Android slot and Rust-owned kind.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRef {
    /// Android snapshot slot.
    pub index: u32,
    /// Rust-owned source kind.
    pub kind: RecoveryFileKind,
}

/// Candidate sorting observations, containing raw identities only in memory.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateInput {
    /// Android snapshot slot.
    pub index: u32,
    /// Previously classified kind.
    pub kind: RecoveryFileKind,
    /// Opaque identity in UTF-16LE; preserves Java ordering and lone surrogates.
    pub source_id_utf16: ByteBuf,
}

/// One raw Java name with an explicit CBOR byte-string field.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameInput {
    /// UTF-16LE code units, including lone surrogates.
    pub utf16: ByteBuf,
}

/// Closed versioned request. Irrelevant fields must be empty.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRequest {
    /// ABI version.
    pub version: u8,
    /// Requested portable policy.
    pub operation: StorageOperation,
    /// Classification chunk.
    #[serde(default)]
    pub observations: Vec<FileObservation>,
    /// Selected candidate identities.
    #[serde(default)]
    pub candidates: Vec<CandidateInput>,
    /// Physical child names, UTF-16LE.
    #[serde(default)]
    pub names: Vec<NameInput>,
    /// Platform modification timestamps, aligned with the original file array.
    #[serde(default)]
    pub modified: Vec<i64>,
    /// Candidate cap or retained-file count; signed cleanup counts retain legacy behavior.
    #[serde(default)]
    pub max_items: i32,
}

/// Complete plan, validated before Android deletes or opens anything.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePlan {
    /// Original file indices, either ordered children or files to delete.
    pub indices: Vec<u32>,
    /// Classified or sorted/capped recovery candidates.
    pub candidates: Vec<CandidateRef>,
    /// Candidate count exceeded the supplied cap.
    pub limit_exceeded: bool,
}

/// Build a fixed-module envelope; input values never appear in failures.
#[must_use]
pub fn build_envelope(request: &StorageRequest) -> ApiEnvelope<StoragePlan> {
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, MODULE, plan),
        Err(message) => ApiEnvelope::err(1, MODULE, RustError::Error(message.to_owned())),
    }
}

/// Execute pure storage rules under hard input bounds.
///
/// # Errors
/// Rejects unknown versions, oversized observations and incoherent operation fields.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "all item counts are bounded by 100000; signed limits are validated or clamped before conversion"
)]
pub fn plan(request: &StorageRequest) -> Result<StoragePlan, &'static str> {
    use StorageOperation::{Candidates, Classify, Cleanup, Names};
    validate_request(request)?;
    let mut result = StoragePlan {
        indices: Vec::new(),
        candidates: Vec::new(),
        limit_exceeded: false,
    };
    match request.operation {
        Classify => {
            for observation in &request.observations {
                if observation.index >= MAX_ITEMS as u32
                    || observation.suffix_utf16.len() > 14
                    || !observation.suffix_utf16.len().is_multiple_of(2)
                {
                    return Err("Invalid recovery file observation");
                }
                let kind = if observation.directory {
                    (!observation.root && observation.has_strict_page)
                        .then_some(RecoveryFileKind::Directory)
                } else {
                    archive_kind(&observation.suffix_utf16)
                };
                if let Some(kind) = kind {
                    result.candidates.push(CandidateRef {
                        index: observation.index,
                        kind,
                    });
                }
            }
        }
        Candidates => {
            if request.max_items < 0
                || request
                    .candidates
                    .iter()
                    .any(|candidate| candidate.index >= MAX_ITEMS as u32)
            {
                return Err("Invalid recovery candidate limit");
            }
            let mut candidates: Vec<_> = request.candidates.iter().collect();
            // Stable legacy input order was directories first, then archives, each in snapshot order.
            candidates.sort_by(|left, right| {
                utf16_compare(&left.source_id_utf16, &right.source_id_utf16)
                    .then_with(|| {
                        (left.kind != RecoveryFileKind::Directory)
                            .cmp(&(right.kind != RecoveryFileKind::Directory))
                    })
                    .then_with(|| left.index.cmp(&right.index))
            });
            result.limit_exceeded = candidates.len() > request.max_items as usize;
            result.candidates = candidates
                .into_iter()
                .take(request.max_items as usize)
                .map(|candidate| CandidateRef {
                    index: candidate.index,
                    kind: candidate.kind,
                })
                .collect();
        }
        Names => {
            result.indices = (0..request.names.len() as u32).collect();
            result.indices.sort_by(|left, right| {
                utf16_compare(
                    &request.names[*left as usize].utf16,
                    &request.names[*right as usize].utf16,
                )
            });
        }
        Cleanup => {
            let mut indices: Vec<_> = (0..request.modified.len() as u32).collect();
            indices.sort_by(|left, right| {
                request.modified[*right as usize].cmp(&request.modified[*left as usize])
            });
            result.indices = indices
                .into_iter()
                .skip(request.max_items.max(0) as usize)
                .collect();
        }
    }
    Ok(result)
}

fn validate_request(request: &StorageRequest) -> Result<(), &'static str> {
    use StorageOperation::{Candidates, Classify, Cleanup, Names};
    if request.version != 1
        || request.observations.len() > MAX_CLASSIFY_ITEMS
        || request.candidates.len() > MAX_ITEMS
        || request.names.len() > MAX_ITEMS
        || request.modified.len() > MAX_ITEMS
        || (request.operation != Classify && !request.observations.is_empty())
        || (request.operation != Candidates && !request.candidates.is_empty())
        || (request.operation != Names && !request.names.is_empty())
        || (request.operation != Cleanup && !request.modified.is_empty())
        || (matches!(request.operation, Classify | Names) && request.max_items != 0)
    {
        return Err("Invalid storage rule request");
    }
    let names = request.names.iter().map(|name| name.utf16.as_ref()).chain(
        request
            .candidates
            .iter()
            .map(|candidate| candidate.source_id_utf16.as_ref()),
    );
    let mut bytes = 0usize;
    for name in names {
        if !name.len().is_multiple_of(2) || name.len() > MAX_NAME_BYTES - bytes {
            return Err("Invalid storage name observations");
        }
        bytes += name.len();
    }
    Ok(())
}

fn utf16_compare(left: &[u8], right: &[u8]) -> Ordering {
    // Do not compare little-endian bytes, UTF-8 or Unicode scalar values.
    left.chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .cmp(
            right
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]])),
        )
}

fn archive_kind(suffix: &[u8]) -> Option<RecoveryFileKind> {
    use RecoveryFileKind::{Cbz, Rar, SevenZip, Tar, TarGz, TarXz, Zip};
    [
        (".tar.gz", TarGz),
        (".tar.xz", TarXz),
        (".cbz", Cbz),
        (".zip", Zip),
        (".rar", Rar),
        (".7z", SevenZip),
        (".tar", Tar),
    ]
    .into_iter()
    .find_map(|(ending, kind)| {
        let offset = suffix.len().checked_sub(ending.len() * 2)?;
        suffix[offset..]
            .chunks_exact(2)
            .zip(ending.bytes())
            .all(|(pair, expected)| {
                let unit = u16::from_le_bytes([pair[0], pair[1]]);
                u8::try_from(unit)
                    .is_ok_and(|unit| unit.is_ascii() && unit.to_ascii_lowercase() == expected)
            })
            .then_some(kind)
    })
}

#[cfg(test)]
mod tests;
