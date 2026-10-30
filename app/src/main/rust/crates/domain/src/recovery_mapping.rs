//! Bounded association of scanned candidates with Android-owned download rows.
//! Paths remain raw UTF-16 for Java equality. Unmatched rows retain the historical
//! unframed SHA-256 identity, including JVM UTF-8 replacement for lone surrogates.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha256};

use crate::contracts::{ApiEnvelope, RustError};
use crate::recovery::RecoveryStatus;

/// Stable response module.
pub const MODULE: &str = "recoveryMapping";
/// Maximum rows in either snapshot.
pub const MAX_ITEMS: usize = 100_000;
/// Aggregate UTF-16 path budget.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// Encoded JNI request ceiling, checked before decoding.
pub const MAX_REQUEST_BYTES: i32 = 32 * 1024 * 1024;

/// Minimal facts from an already inspected candidate, in scan order.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Optional gallery identity.
    pub gid: Option<i64>,
    /// Inspected status; only ready candidates can match.
    pub status: RecoveryStatus,
    /// Existing canonical source identity.
    pub source_key: String,
    /// Resolved promotion path, with Java null distinct from the empty string.
    pub path_utf16: Option<ByteBuf>,
}

/// One immutable Room download observation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    /// Persisted signed gallery identifier, validated by the conflict owner later.
    pub gid: i64,
    /// Raw Java dirname; null only collapses to empty when hashing an unmatched row.
    pub dirname_utf16: Option<ByteBuf>,
}

/// Versioned batch; Android owns all object and transaction lifetimes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Exactly one.
    pub version: u8,
    /// Candidates in their original first-match order.
    pub candidates: Vec<Candidate>,
    /// Downloads in their original order, including duplicates.
    pub downloads: Vec<Download>,
}

/// One source identity for the corresponding original download row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    /// Matching original candidate slot, or none for a database-only identity.
    pub candidate_index: Option<usize>,
    /// Canonical source key or historical database-only SHA-256.
    pub source_key: String,
}

/// Complete source-aligned association, never a partial result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Plan {
    /// Exactly one mapping per input download.
    pub mappings: Vec<Mapping>,
}

/// Construct a typed envelope without including path observations in errors.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, MODULE, plan),
        Err(message) => ApiEnvelope::err(1, MODULE, RustError::Error(message.into())),
    }
}

/// Associate by exact nullable Java path and gid, preserving the first candidate.
///
/// # Errors
/// Rejects unsupported versions, malformed source keys/UTF-16 byte arrays and budgets.
pub fn plan(request: &Request) -> Result<Plan, &'static str> {
    if request.version != 1
        || request.candidates.len() > MAX_ITEMS
        || request.downloads.len() > MAX_ITEMS
        || request.candidates.iter().any(|candidate| {
            candidate.source_key.len() != 64
                || !candidate
                    .source_key
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err("Invalid recovery mapping request");
    }
    let paths = request
        .candidates
        .iter()
        .filter_map(|candidate| candidate.path_utf16.as_deref())
        .chain(
            request
                .downloads
                .iter()
                .filter_map(|row| row.dirname_utf16.as_deref()),
        );
    let mut bytes = 0;
    for path in paths {
        if !path.len().is_multiple_of(2) || path.len() > MAX_TEXT_BYTES - bytes {
            return Err("Invalid recovery mapping paths");
        }
        bytes += path.len();
    }
    let mut first = BTreeMap::new();
    for (index, candidate) in request.candidates.iter().enumerate() {
        if candidate.status == RecoveryStatus::Ready
            && let Some(gid) = candidate.gid
        {
            first
                .entry((gid, candidate.path_utf16.as_deref()))
                .or_insert(index);
        }
    }
    let mappings = request
        .downloads
        .iter()
        .map(|download| {
            let candidate_index = first
                .get(&(download.gid, download.dirname_utf16.as_deref()))
                .copied();
            Mapping {
                candidate_index,
                source_key: candidate_index.map_or_else(
                    || database_key(download),
                    |index| request.candidates[index].source_key.clone(),
                ),
            }
        })
        .collect();
    Ok(Plan { mappings })
}

fn database_key(download: &Download) -> String {
    let mut digest = Sha256::new();
    digest.update(format!("database:{}:", download.gid).as_bytes());
    let path = download
        .dirname_utf16
        .as_ref()
        .map_or(&[][..], |path| path.as_ref());
    let units = path
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    for scalar in char::decode_utf16(units) {
        let mut encoded = [0; 4];
        // String.getBytes(UTF_8), used by Kotlin encodeToByteArray, replaces each
        // malformed UTF-16 unit with ASCII '?', not the UTF-8 encoding of U+FFFD.
        digest.update(scalar.unwrap_or('?').encode_utf8(&mut encoded).as_bytes());
    }
    let mut result = String::with_capacity(64);
    for byte in digest.finalize() {
        let _ = write!(result, "{byte:02x}");
    }
    result
}

#[cfg(test)]
mod tests;
