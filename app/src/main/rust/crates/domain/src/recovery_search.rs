//! Recovery HTTP throttling and FIFO cache effects, without networking or gallery objects.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// V1 command facts. Cached gallery models stay in Android.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySearchRequest {
    /// Contract version.
    pub version: u8,
    /// Whether the normalized key already has a cached result.
    pub cached: bool,
    /// Monotonic elapsed milliseconds since the prior request.
    pub elapsed_millis: i64,
    /// Size before inserting a successful result.
    pub cache_size: i32,
    /// Whether the new network result contains any galleries.
    pub has_results: bool,
}

/// Validated platform effects for one scheduling or cache observation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySearchPlan {
    /// Use the cached result and do not perform HTTP.
    pub use_cache: bool,
    /// Delay before starting a new HTTP request.
    pub wait_millis: i64,
    /// Store only nonempty results after a successful HTTP request.
    pub store: bool,
    /// Number of oldest keys to evict after inserting.
    pub evict_count: i32,
}

/// Plans the legacy three-second interval and 128-entry insertion-order cache.
#[must_use]
pub fn build_envelope(request: &RecoverySearchRequest) -> ApiEnvelope<RecoverySearchPlan> {
    if request.version != 1 || !(0..=128).contains(&request.cache_size) {
        return ApiEnvelope::err(
            1,
            "recoverySearch",
            RustError::Error("Invalid recovery search input".into()),
        );
    }
    let store = !request.cached && request.has_results;
    ApiEnvelope::ok(
        1,
        "recoverySearch",
        RecoverySearchPlan {
            use_cache: request.cached,
            wait_millis: if request.cached {
                0
            } else {
                3_000_i64
                    .saturating_sub(request.elapsed_millis)
                    .clamp(0, 3_000)
            },
            store,
            evict_count: i32::from(store && request.cache_size == 128),
        },
    )
}

#[cfg(test)]
mod tests;
