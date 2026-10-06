//! Bounded preview pagination and ordered sparse token updates; no token-map snapshot.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap as _};

/// Maximum preview facts in one response plan.
pub const MAX_PREVIEWS: usize = 100_000;
/// Maximum UTF-8 bytes per opaque token.
pub const MAX_TOKEN_BYTES: usize = 65_536;
/// Maximum aggregate token bytes per response plan.
pub const MAX_TOTAL_TOKEN_BYTES: usize = 1024 * 1024;
/// Maximum encoded request bytes at JNI.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
/// Maximum encoded envelope bytes at JNI.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Only facts consumed by the policy; URLs and existing token maps stay in Android.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewToken {
    /// Signed legacy page position, including negative values.
    pub position: i32,
    /// Opaque token; empty entries leave existing map rows unchanged.
    pub p_token: String,
}

/// One bounded pagination or response operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PreviewTokenRequest {
    /// Exact supported protocol version.
    pub version: u8,
    /// Complete operation, never one call per token.
    #[serde(flatten)]
    pub operation: PreviewTokenOperation,
}

// serde flatten serializes an indefinite map; pin the shared definite-length wire instead.
impl Serialize for PreviewTokenRequest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("version", &self.version)?;
        match &self.operation {
            PreviewTokenOperation::Lookup {
                index,
                preview_per_page,
            } => {
                map.serialize_entry("kind", "lookup")?;
                map.serialize_entry("index", index)?;
                map.serialize_entry("previewPerPage", preview_per_page)?;
            }
            PreviewTokenOperation::Apply {
                preview_index,
                previews,
            } => {
                map.serialize_entry("kind", "apply")?;
                map.serialize_entry("previewIndex", preview_index)?;
                map.serialize_entry("previews", previews)?;
            }
        }
        map.end()
    }
}

/// Closed operation shapes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PreviewTokenOperation {
    /// Determine the HTTP preview page from current metadata.
    Lookup {
        /// Requested image index with JVM signed Int semantics.
        index: i32,
        /// Current preview size; non-positive means page zero.
        preview_per_page: i32,
    },
    /// Plan one completed preview response, regardless of response age.
    Apply {
        /// Requested preview page, not an image index.
        preview_index: i32,
        /// Source-order facts, including duplicates and empty tokens.
        previews: Vec<PreviewToken>,
    },
}

/// Complete decision before Android effects; updates refer to request facts only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewTokenPlan {
    /// Page for the existing gallery-detail request planner.
    Lookup {
        /// Signed preview page.
        preview_index: i32,
    },
    /// New size plus ordered sparse source indices to apply to the current map.
    Apply {
        /// Zero on an empty first page; otherwise legacy signed quotient/size.
        preview_per_page: i32,
        /// Strictly increasing indices of nonempty tokens, retaining duplicates.
        updates: Vec<usize>,
    },
}

/// Plans pagination or sparse deltas without reading an existing token map.
///
/// # Errors
/// Returns fixed typed errors for version, budgets, or empty nonzero preview pages.
pub fn plan(request: &PreviewTokenRequest) -> Result<PreviewTokenPlan, RustError> {
    if request.version != 1 {
        return Err(invalid());
    }
    match &request.operation {
        PreviewTokenOperation::Lookup {
            index,
            preview_per_page,
        } => Ok(PreviewTokenPlan::Lookup {
            preview_index: if *preview_per_page > 0 {
                index / preview_per_page
            } else {
                0
            },
        }),
        PreviewTokenOperation::Apply {
            preview_index,
            previews,
        } => {
            if previews.len() > MAX_PREVIEWS
                || previews
                    .iter()
                    .any(|row| row.p_token.len() > MAX_TOKEN_BYTES)
                || previews.iter().fold(0_usize, |total, row| {
                    total.saturating_add(row.p_token.len())
                }) > MAX_TOTAL_TOKEN_BYTES
            {
                return Err(invalid());
            }
            let preview_per_page = if *preview_index == 0 {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_possible_wrap,
                    reason = "count checked against 100000-row budget above, below i32::MAX"
                )]
                let count = previews.len() as i32;
                count
            } else {
                let first = previews
                    .first()
                    .ok_or_else(|| RustError::Error("Empty nonzero preview page".into()))?;
                // JVM Int division wraps MIN_VALUE / -1 rather than panicking.
                first.position.wrapping_div(*preview_index)
            };
            Ok(PreviewTokenPlan::Apply {
                preview_per_page,
                updates: previews
                    .iter()
                    .enumerate()
                    .filter_map(|(index, row)| (!row.p_token.is_empty()).then_some(index))
                    .collect(),
            })
        }
    }
}

fn invalid() -> RustError {
    RustError::Error("Invalid preview token request".into())
}

/// Wraps one policy decision in the exact V1 module envelope.
#[must_use]
pub fn build_envelope(request: &PreviewTokenRequest) -> ApiEnvelope<PreviewTokenPlan> {
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, "previewTokens", plan),
        Err(error) => ApiEnvelope::err(1, "previewTokens", error),
    }
}

#[cfg(test)]
mod tests;
