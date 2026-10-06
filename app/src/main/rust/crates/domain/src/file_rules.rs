//! Portable filename rules preserving Android's UTF-16 and UTF-8 replacement semantics.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

const MAX_INPUT_UNITS: usize = 65_536;
const MAX_FILENAME_BYTES: usize = 200;

/// Closed filename operations; paths and filesystem effects never enter this contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileRuleKind {
    /// Replace FAT-invalid characters and shorten long names through their middle.
    Sanitize,
    /// Return the nonempty suffix after the last dot.
    Extension,
    /// Return the nonempty prefix before the last dot, or the complete name.
    Basename,
}

/// Bounded V1 filename observation, expressed as UTF-16 to preserve lone surrogates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRulesRequest {
    /// Contract version.
    pub version: u8,
    /// Selected operation.
    pub kind: FileRuleKind,
    /// Unmodified Java string units; absent names are valid for extraction operations.
    pub units: Option<Vec<u16>>,
}

/// Complete operation-tagged result, without any platform file object.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRulesPlan {
    /// Echoes the requested operation.
    pub kind: FileRuleKind,
    /// Result string units, or no basename/extension.
    pub units: Option<Vec<u16>>,
}

/// Applies one filename operation with bounded allocation and fixed input-free failures.
#[must_use]
pub fn build_envelope(request: &FileRulesRequest) -> ApiEnvelope<FileRulesPlan> {
    if request.version != 1
        || request
            .units
            .as_ref()
            .is_some_and(|v| v.len() > MAX_INPUT_UNITS)
    {
        return ApiEnvelope::err(
            1,
            "fileRules",
            RustError::Error("Invalid file rules input".into()),
        );
    }
    let units = match request.kind {
        FileRuleKind::Sanitize => {
            let Some(input) = request.units.as_deref() else {
                return invalid_filename();
            };
            let end = input
                .iter()
                .rposition(|v| !matches!(v, 32 | 46))
                .map_or(0, |i| i + 1);
            if end == 0 {
                return invalid_filename();
            }
            let name = input[..end]
                .iter()
                .map(|&v| {
                    if matches!(
                        v,
                        0..=31 | 34 | 42 | 47 | 58 | 60 | 62 | 63 | 92 | 124 | 127
                    ) {
                        95
                    } else {
                        v
                    }
                })
                .collect::<Vec<_>>();
            Some(shorten(&name))
        }
        FileRuleKind::Extension => request.units.as_deref().and_then(|input| {
            let dot = input.iter().rposition(|&v| v == 46)?;
            (!input[dot + 1..].is_empty()).then(|| input[dot + 1..].to_vec())
        }),
        FileRuleKind::Basename => request.units.as_deref().and_then(|input| {
            let end = input.iter().rposition(|&v| v == 46).unwrap_or(input.len());
            (end > 0).then(|| input[..end].to_vec())
        }),
    };
    ApiEnvelope::ok(
        1,
        "fileRules",
        FileRulesPlan {
            kind: request.kind,
            units,
        },
    )
}

fn invalid_filename() -> ApiEnvelope<FileRulesPlan> {
    ApiEnvelope::err(1, "fileRules", RustError::Error("Invalid filename".into()))
}

fn is_pair(high: u16, low: u16) -> bool {
    (0xd800..=0xdbff).contains(&high) && (0xdc00..=0xdfff).contains(&low)
}

// Java's UTF-8 encoder replaces each lone surrogate with one ASCII question mark.
fn unit_bytes(unit: u16) -> usize {
    match unit {
        0..=0x7f | 0xd800..=0xdfff => 1,
        0x80..=0x7ff => 2,
        _ => 3,
    }
}

fn shorten(name: &[u16]) -> Vec<u16> {
    let mut prefix = Vec::with_capacity(name.len() + 1);
    prefix.push(0usize);
    for (i, &unit) in name.iter().enumerate() {
        let size = if i > 0 && is_pair(name[i - 1], unit) {
            3
        } else {
            unit_bytes(unit)
        };
        prefix.push(prefix[i] + size);
    }
    if prefix[name.len()] <= MAX_FILENAME_BYTES {
        return name.to_vec();
    }
    let mut suffix = vec![0usize; name.len() + 1];
    for i in (0..name.len()).rev() {
        let size = if i + 1 < name.len() && is_pair(name[i], name[i + 1]) {
            3
        } else {
            unit_bytes(name[i])
        };
        suffix[i] = suffix[i + 1] + size;
    }
    // Repeated deletion at length / 2 removes one contiguous middle interval.
    // Prefix/suffix byte tables make the original quadratic loop linear while
    // retaining exactly its UTF-16 deletions, including split surrogate pairs.
    let mut left = name.len() / 2;
    let mut right = left;
    loop {
        let boundary =
            usize::from(left > 0 && right < name.len() && is_pair(name[left - 1], name[right])) * 2;
        if prefix[left] + suffix[right] + boundary <= MAX_FILENAME_BYTES - 3 {
            break;
        }
        let middle = (left + name.len() - right) / 2;
        if middle < left {
            left -= 1;
        } else {
            right += 1;
        }
    }
    let mut result = name[..left].to_vec();
    result.extend_from_slice(&name[right..]);
    let middle = result.len() / 2;
    result.splice(middle..middle, [46, 46, 46]);
    result
}

#[cfg(test)]
mod tests;
