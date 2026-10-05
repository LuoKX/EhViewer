//! Bounded current-CBOR and legacy `SpiderInfo` codecs shared by reader and recovery.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::BTreeMap;

/// Maximum metadata snapshot retained at this boundary.
pub const MAX_BYTES: usize = 512 * 1024;

/// Complete reader metadata; token rows are preserved even before all pages are known.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpiderInfo {
    /// Gallery ID, unchanged from the stored format.
    pub gid: i64,
    /// Public gallery token, never included in diagnostics.
    pub token: String,
    /// Declared page count.
    pub pages: i32,
    /// Complete zero-based page-token map; absent means empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub p_token_map: BTreeMap<i32, String>,
    /// Preview page size; absent means minus one.
    #[serde(
        default = "unknown_preview_size",
        skip_serializing_if = "is_unknown_preview_size"
    )]
    pub preview_per_page: i32,
}

/// Versioned complete codec operation.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpiderInfoRequest {
    /// Decode current CBOR, optionally recognizing the legacy file grammar.
    Decode {
        /// Bounded source bytes.
        bytes: ByteBuf,
        /// Compatibility enabled for on-disk files, disabled for the current cache.
        allow_legacy: bool,
    },
    /// Encode the same unversioned storage map read by prior Android releases.
    Encode {
        /// Complete snapshot copied under Android's token-map lock.
        info: SpiderInfo,
    },
}

/// V1 codec result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SpiderInfoPlan {
    /// Entire decoded snapshot.
    Decode {
        /// Complete reader metadata.
        info: SpiderInfo,
    },
    /// Compatible encoded storage bytes.
    Encode {
        /// Bounded CBOR map, not an envelope.
        bytes: ByteBuf,
    },
}

const fn unknown_preview_size() -> i32 {
    -1
}
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip predicate requires a reference"
)]
fn is_unknown_preview_size(value: &i32) -> bool {
    *value == -1
}

/// Decodes one bounded snapshot without hiding native failures.
///
/// # Errors
/// Returns fixed, input-free errors for oversize or malformed storage.
pub fn decode(bytes: &[u8], allow_legacy: bool) -> Result<SpiderInfo, RustError> {
    bounded(bytes)?;
    serde_cbor::from_slice(bytes)
        .ok()
        .or_else(|| allow_legacy.then(|| legacy(bytes, true)).flatten())
        .ok_or_else(invalid)
}

/// Encodes all token rows using the prior field names and default omission rules.
///
/// # Errors
/// Rejects malformed serialization or encoded snapshots beyond the metadata cap.
#[allow(
    clippy::missing_panics_doc,
    reason = "closed storage struct has only infallible CBOR primitive serializers"
)]
pub fn encode(info: &SpiderInfo) -> Result<Vec<u8>, RustError> {
    let bytes =
        serde_cbor::to_vec(info).expect("SpiderInfo storage primitives serialize infallibly");
    bounded(&bytes)?;
    Ok(bytes)
}

/// Extracts the historical recovery identity, ignoring unrelated CBOR token-map fields.
///
/// # Errors
/// Rejects malformed or oversized input. Recovery validates identity/page semantics.
pub fn decode_identity(bytes: &[u8]) -> Result<(i64, String, i32), RustError> {
    #[derive(Deserialize)]
    struct Identity {
        gid: i64,
        token: String,
        pages: i32,
    }
    bounded(bytes)?;
    if let Ok(info) = serde_cbor::from_slice::<Identity>(bytes) {
        return Ok((info.gid, info.token, info.pages));
    }
    let info = legacy(bytes, false).ok_or_else(invalid)?;
    Ok((info.gid, info.token, info.pages))
}

fn legacy(bytes: &[u8], strict: bool) -> Option<SpiderInfo> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = if strict {
        text.split_inclusive('\n')
            .filter_map(|line| {
                line.strip_suffix('\n')
                    .map(|line| line.strip_suffix('\r').unwrap_or(line))
            })
            .collect::<Vec<_>>()
            .into_iter()
    } else {
        text.lines().collect::<Vec<_>>().into_iter()
    };
    lines.next()?;
    lines.next()?;
    let gid = lines.next()?.parse().ok()?;
    let token = lines.next()?.to_owned();
    lines.next()?;
    lines.next()?;
    let preview_per_page = lines.next()?.parse().ok()?;
    let pages = lines.next()?.parse().ok()?;
    let mut info = SpiderInfo {
        gid,
        token,
        pages,
        p_token_map: BTreeMap::new(),
        preview_per_page,
    };
    for line in lines {
        if let Some((index, token)) = line.split_once(' ') {
            if index.is_empty() {
                continue;
            }
            let Ok(index) = index.parse() else {
                break;
            };
            if !token.is_empty() {
                info.p_token_map.insert(index, token.to_owned());
            }
        }
    }
    Some(info)
}

fn bounded(bytes: &[u8]) -> Result<(), RustError> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn invalid() -> RustError {
    RustError::Error("Invalid SpiderInfo metadata".into())
}

/// Produces the complete V1 codec envelope.
#[must_use]
#[allow(
    clippy::if_not_else,
    reason = "reject unsupported wire versions before codec work"
)]
pub fn build_envelope(version: u8, request: &SpiderInfoRequest) -> ApiEnvelope<SpiderInfoPlan> {
    let result = if version != 1 {
        Err(invalid())
    } else {
        match request {
            SpiderInfoRequest::Decode {
                bytes,
                allow_legacy,
            } => decode(bytes, *allow_legacy).map(|info| SpiderInfoPlan::Decode { info }),
            SpiderInfoRequest::Encode { info } => {
                encode(info).map(|bytes| SpiderInfoPlan::Encode {
                    bytes: bytes.into(),
                })
            }
        }
    };
    match result {
        Ok(plan) => ApiEnvelope::ok(1, "spiderInfo", plan),
        Err(error) => ApiEnvelope::err(1, "spiderInfo", error),
    }
}

#[cfg(test)]
mod tests;
