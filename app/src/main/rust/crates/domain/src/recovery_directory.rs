//! Bounded directory observations and recovery promotion policy; no file access.
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::contracts::{ApiEnvelope, RustError};
use crate::storage_rules::RecoveryFileKind;

/// Exact versioned envelope module.
pub const MODULE: &str = "recoveryDirectory";
/// Maximum facts in one meaningful plan.
pub const MAX_ITEMS: usize = 100_000;
/// Aggregate UTF-16LE byte budget.
pub const MAX_TEXT_BYTES: usize = 16 * 1024 * 1024;
/// Maximum encoded request size checked before decoding by JNI.
pub const MAX_REQUEST_BYTES: i32 = 32 * 1024 * 1024;

/// Closed policy operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Operation {
    /// Stable ordering, sidecar selection and fingerprint input aggregation.
    Observe,
    /// Batch promotion eligibility and relative parent decisions.
    Promote,
}

/// Caller-observed directory metadata, never a platform object.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Child {
    /// Raw Java identity code units; lone surrogates are preserved.
    pub source_id_utf16: ByteBuf,
    /// Raw Java filename code units.
    pub name_utf16: ByteBuf,
    /// Signed JVM size, including provider anomalies.
    pub size: Option<i64>,
    /// Signed JVM timestamp, including negative values.
    pub modified_at_millis: Option<i64>,
}

/// Promotion facts. Android retains path resolution and transaction checks.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Promotion {
    /// Raw root-relative Java path; Rust never opens it.
    pub relative_path_utf16: ByteBuf,
    /// Closed source type shared with storage classification.
    pub kind: RecoveryFileKind,
    /// Candidate identity, not required to be valid.
    pub gid: Option<i64>,
}

/// Versioned batch request; irrelevant operation fields must be empty.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Exactly version one.
    pub version: u8,
    /// Selected policy.
    pub operation: Operation,
    /// Original directory snapshot order.
    pub children: Vec<Child>,
    /// Original promotion snapshot order.
    pub promotions: Vec<Promotion>,
}

/// Closed relative-path selection, not an arbitrary native path.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathDecision {
    /// No eligible path.
    None,
    /// Original directory relative path.
    Relative,
    /// Prefix ending before final slash of original archive relative path.
    Parent,
}

/// One promotion decision aligned with request source index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotionPlan {
    /// Original promotion slot.
    pub index: u32,
    /// Closed original-path selection.
    pub decision: PathDecision,
    /// UTF-16 prefix length; zero for ineligible paths.
    pub end: u32,
}

/// Complete plan, validated before Android sidecar or persistence effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// All child source indices in stable Java identity order.
    pub indices: Vec<u32>,
    /// First matching sidecar in ordered children, including directory facts.
    pub spider_index: Option<u32>,
    /// First matching comic sidecar in ordered children.
    pub comic_index: Option<u32>,
    /// JVM Long wrapping sum, null size contributes zero.
    pub total_size: i64,
    /// Maximum known signed timestamp; null only when all missing.
    pub modified_at_millis: Option<i64>,
    /// All promotion decisions aligned with source order.
    pub promotions: Vec<PromotionPlan>,
}

/// Fixed typed, input-free failures and exact success envelope.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    match plan(request) {
        Ok(value) => ApiEnvelope::ok(1, MODULE, value),
        Err(message) => ApiEnvelope::err(1, MODULE, RustError::Error(message.to_owned())),
    }
}

/// Execute bounded portable policy.
///
/// # Errors
/// Rejects version, operation-field, count, UTF-16 and aggregate byte violations.
#[allow(
    clippy::cast_possible_truncation,
    reason = "counts and text lengths are bounded below u32"
)]
pub fn plan(request: &Request) -> Result<Plan, &'static str> {
    validate_request(request)?;
    let mut result = Plan {
        indices: Vec::new(),
        spider_index: None,
        comic_index: None,
        total_size: 0,
        modified_at_millis: None,
        promotions: Vec::new(),
    };
    match request.operation {
        Operation::Observe => {
            result.indices = (0..request.children.len() as u32).collect();
            // Stable sort is essential when provider identities compare equal.
            result.indices.sort_by(|left, right| {
                units(&request.children[*left as usize].source_id_utf16)
                    .cmp(units(&request.children[*right as usize].source_id_utf16))
            });
            for &index in &result.indices {
                let child = &request.children[index as usize];
                if result.spider_index.is_none() && matches_ascii(&child.name_utf16, ".ehviewer") {
                    result.spider_index = Some(index);
                }
                if result.comic_index.is_none() && matches_ascii(&child.name_utf16, "comicinfo.xml")
                {
                    result.comic_index = Some(index);
                }
                result.total_size = result.total_size.wrapping_add(child.size.unwrap_or(0));
                if let Some(modified) = child.modified_at_millis {
                    result.modified_at_millis = Some(
                        result
                            .modified_at_millis
                            .map_or(modified, |old| old.max(modified)),
                    );
                }
            }
        }
        Operation::Promote => {
            result.promotions = request
                .promotions
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let path: Vec<_> = units(&item.relative_path_utf16).collect();
                    let mut decision = PathDecision::None;
                    let mut end = 0;
                    if let Some(gid) = item.gid.filter(|gid| *gid > 0) {
                        if item.kind == RecoveryFileKind::Directory {
                            if !blank(&path) {
                                decision = PathDecision::Relative;
                                end = path.len();
                            }
                        } else if matches!(item.kind, RecoveryFileKind::Cbz | RecoveryFileKind::Zip)
                        {
                            let slash = path.iter().rposition(|unit| *unit == u16::from(b'/'));
                            let start = slash.map_or(0, |position| position + 1);
                            let extension = if item.kind == RecoveryFileKind::Cbz {
                                "cbz"
                            } else {
                                "zip"
                            };
                            let name = &item.relative_path_utf16[start * 2..];
                            if matches_ascii(name, &format!("{gid}.{extension}"))
                                && let Some(position) = slash
                                && !blank(&path[..position])
                            {
                                decision = PathDecision::Parent;
                                end = position;
                            }
                        }
                    }
                    PromotionPlan {
                        index: index as u32,
                        decision,
                        end: end as u32,
                    }
                })
                .collect();
        }
    }
    Ok(result)
}

fn validate_request(request: &Request) -> Result<(), &'static str> {
    if request.version != 1
        || request.children.len() > MAX_ITEMS
        || request.promotions.len() > MAX_ITEMS
        || (request.operation != Operation::Observe && !request.children.is_empty())
        || (request.operation != Operation::Promote && !request.promotions.is_empty())
    {
        return Err("Invalid recovery directory request");
    }
    let texts = request
        .children
        .iter()
        .flat_map(|child| [&child.source_id_utf16, &child.name_utf16])
        .chain(
            request
                .promotions
                .iter()
                .map(|item| &item.relative_path_utf16),
        );
    let mut bytes = 0;
    for text in texts {
        if !text.len().is_multiple_of(2) || text.len() > MAX_TEXT_BYTES - bytes {
            return Err("Invalid recovery directory observations");
        }
        bytes += text.len();
    }
    Ok(())
}

fn units(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
}

fn matches_ascii(bytes: &[u8], expected: &str) -> bool {
    // Java equals(ignoreCase=true) uses per-code-unit upper/lower case, not full
    // Unicode folding. For these ASCII literals only I has non-ASCII partners.
    bytes.len() == expected.len() * 2
        && units(bytes).zip(expected.bytes()).all(|(unit, ascii)| {
            (ascii == b'i' && matches!(unit, 0x0130 | 0x0131))
                || u8::try_from(unit).is_ok_and(|value| value.to_ascii_lowercase() == ascii)
        })
}

fn blank(path: &[u16]) -> bool {
    // Kotlin Char.isWhitespace = Java isWhitespace || isSpaceChar. In particular
    // NBSP is blank, U+0085/U+180E are not. No Rust trim/Unicode-version dependency.
    path.iter().all(|unit| {
        matches!(*unit,
        0x0009..=0x000d | 0x001c..=0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a
        | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000)
    })
}

#[cfg(test)]
mod tests;
