//! Update eligibility and binary selection. HTTP, credentials and persistence stay on Android.
use crate::contracts::RustError;
use serde::{Deserialize, Serialize};

/// Maximum binary/workflow observations in one plan.
pub const MAX_ITEMS: usize = 4096;
/// Maximum UTF-8 bytes in one scalar observation.
pub const MAX_TEXT: usize = 65_536;
/// Kotlin Instant's inclusive epoch-second bounds.
pub const MIN_SECOND: i64 = -31_557_014_167_219_200;
/// Kotlin Instant's inclusive maximum epoch second.
pub const MAX_SECOND: i64 = 31_556_889_864_403_199;
/// Fixed failure; never includes remote payloads.
pub const INVALID_INPUT: &str = "Invalid update policy input";
/// Repaired empty release behavior.
pub const EMPTY_RELEASE: &str = "No update release assets available";
/// Repaired empty artifact behavior.
pub const EMPTY_ARTIFACTS: &str = "No update workflow artifacts available";
/// Repaired empty workflow behavior.
pub const EMPTY_WORKFLOWS: &str = "No successful update workflow runs available";

/// One meaningful operation; selection remains batched.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateOperation {
    /// Check clock/interval/force facts.
    Check,
    /// Match one binary name using flavor and first supported ABI.
    Match,
    /// Prepare server-ordered lazy custom predicate evaluation.
    PrepareRelease,
    /// Prepare UTF-16-ordered lazy custom predicate evaluation.
    PrepareArtifacts,
    /// Select a server-ordered release asset.
    SelectRelease,
    /// Select an artifact in stable UTF-16 name order.
    SelectArtifacts,
    /// Select the first server-ordered workflow run.
    SelectWorkflow,
    /// Compare release strings, not `SemVer`.
    CompareRelease,
    /// Compare the first seven UTF-16 SHA units, not full hashes.
    CompareWorkflow,
}
/// Bounded immutable facts. Custom helper predicates are observations, not Rust callbacks.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePolicyRequest {
    /// Contract version.
    pub version: u8,
    /// Requested policy.
    pub operation: UpdateOperation,
    /// Current clock epoch seconds.
    pub now_seconds: i64,
    /// Current clock nanoseconds within second.
    pub now_nanos: u32,
    /// Persisted epoch seconds; Kotlin Instant clamps out-of-range values.
    pub last_seconds: i64,
    /// Signed Kotlin Int preference, including negative values.
    pub interval_days: i32,
    /// Explicit forced check.
    pub force_check: bool,
    /// Case-sensitive build flavor substring.
    pub flavor: String,
    /// Android ABI order; only its first item participates.
    pub abis: Vec<String>,
    /// Server-ordered names or workflow identities.
    pub names: Vec<String>,
    /// Optional lazy predicate results: a continuous prefix of the prepared order.
    pub matches: Option<Vec<bool>>,
    /// Installed version or short SHA.
    pub current: String,
    /// Latest version or full SHA.
    pub latest: String,
}
/// Closed execution outcomes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateDecision {
    /// No HTTP or persistence effects.
    Skip,
    /// Persist captured clock, then execute chosen channel's HTTP.
    Check,
    /// Match helper result.
    Match,
    /// Execute a validated lazy callback evaluation order.
    Observe,
    /// Selected source index.
    Selected,
    /// Identical installed version/SHA.
    NoUpdate,
    /// Different version/SHA; never implies `SemVer` ordering.
    Candidate,
}
/// Fully shaped plan; UTF-16 units preserve Kotlin `take(7)` even at a surrogate boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePolicyPlan {
    /// Closed outcome.
    pub decision: UpdateDecision,
    /// Source index, present only for selection.
    pub index: Option<usize>,
    /// Exact compared version units, present only for comparisons.
    pub version_units: Option<Vec<u16>>,
    /// Match result, present only for matching.
    pub matched: Option<bool>,
    /// Complete source permutation, present only for custom predicate preparation.
    pub order: Option<Vec<usize>>,
}

/// Validate and plan with body-free errors.
///
/// # Errors
/// Rejects unsupported versions, budgets, inconsistent predicate facts, or empty selections.
pub fn plan(request: &UpdatePolicyRequest) -> Result<UpdatePolicyPlan, RustError> {
    validate(request)?;
    let mut result = UpdatePolicyPlan {
        decision: UpdateDecision::Skip,
        index: None,
        version_units: None,
        matched: None,
        order: None,
    };
    match request.operation {
        UpdateOperation::Check => {
            // Instant.fromEpochSeconds and Instant + Int.days saturate, never JVM-wrap.
            let last = clamp_instant(i128::from(request.last_seconds), 0);
            let threshold = clamp_instant(
                i128::from(last.0) + i128::from(request.interval_days) * 86_400,
                last.1,
            );
            if request.force_check
                || (request.interval_days != 0
                    && (request.now_seconds, request.now_nanos) > threshold)
            {
                result.decision = UpdateDecision::Check;
            }
        }
        UpdateOperation::Match => {
            if request.names.len() != 1 || request.matches.is_some() {
                return Err(error(INVALID_INPUT));
            }
            result.decision = UpdateDecision::Match;
            result.matched = Some(matches_variant(&request.names[0], request));
        }
        UpdateOperation::PrepareRelease | UpdateOperation::PrepareArtifacts => {
            if request.matches.is_some() {
                return Err(error(INVALID_INPUT));
            }
            result.order = Some(selection_order(request)?);
            result.decision = UpdateDecision::Observe;
        }
        UpdateOperation::SelectRelease
        | UpdateOperation::SelectArtifacts
        | UpdateOperation::SelectWorkflow => {
            result.index = Some(select(request)?);
            result.decision = UpdateDecision::Selected;
        }
        UpdateOperation::CompareRelease | UpdateOperation::CompareWorkflow => {
            let mut units: Vec<u16> = request.latest.encode_utf16().collect();
            if request.operation == UpdateOperation::CompareWorkflow {
                units.truncate(7);
            }
            result.decision = if request.current.encode_utf16().eq(units.iter().copied()) {
                UpdateDecision::NoUpdate
            } else {
                UpdateDecision::Candidate
            };
            result.version_units = Some(units);
        }
    }
    Ok(result)
}
fn validate(request: &UpdatePolicyRequest) -> Result<(), RustError> {
    if request.version != 1
        || !(MIN_SECOND..=MAX_SECOND).contains(&request.now_seconds)
        || request.now_nanos >= 1_000_000_000
        || request.names.len() > MAX_ITEMS
        || request.abis.len() > 32
        || request
            .matches
            .as_ref()
            .is_some_and(|values| values.len() > request.names.len())
        || [&request.flavor, &request.current, &request.latest]
            .into_iter()
            .chain(&request.names)
            .chain(&request.abis)
            .any(|value| value.len() > MAX_TEXT)
    {
        return Err(error(INVALID_INPUT));
    }
    Ok(())
}
fn clamp_instant(seconds: i128, nanos: u32) -> (i64, u32) {
    if seconds < i128::from(MIN_SECOND) {
        (MIN_SECOND, 0)
    } else if seconds > i128::from(MAX_SECOND) {
        (MAX_SECOND, 999_999_999)
    } else {
        (
            i64::try_from(seconds).expect("bounded Instant seconds"),
            nanos,
        )
    }
}
fn matches_variant(name: &str, request: &UpdatePolicyRequest) -> bool {
    let abi = request.abis.first().map_or("universal", String::as_str);
    let abi = if matches!(abi, "arm64-v8a" | "x86_64" | "armeabi-v7a") {
        abi
    } else {
        "universal"
    };
    name.contains(&request.flavor) && name.contains(abi)
}
fn selection_order(request: &UpdatePolicyRequest) -> Result<Vec<usize>, RustError> {
    if request.names.is_empty() {
        return Err(error(match request.operation {
            UpdateOperation::PrepareRelease | UpdateOperation::SelectRelease => EMPTY_RELEASE,
            UpdateOperation::PrepareArtifacts | UpdateOperation::SelectArtifacts => EMPTY_ARTIFACTS,
            _ => EMPTY_WORKFLOWS,
        }));
    }
    let mut order: Vec<usize> = (0..request.names.len()).collect();
    if matches!(
        request.operation,
        UpdateOperation::PrepareArtifacts | UpdateOperation::SelectArtifacts
    ) {
        // Stable JVM UTF-16 ordering, including duplicate names.
        order.sort_by(|&left, &right| {
            request.names[left]
                .encode_utf16()
                .cmp(request.names[right].encode_utf16())
        });
    }
    Ok(order)
}
fn select(request: &UpdatePolicyRequest) -> Result<usize, RustError> {
    let order = selection_order(request)?;
    if request.operation == UpdateOperation::SelectWorkflow {
        if request.matches.is_some() {
            return Err(error(INVALID_INPUT));
        }
        return Ok(0);
    }
    if let Some(values) = &request.matches {
        // A true observation must terminate evaluation; all-false fallback requires full coverage.
        if values.is_empty()
            || values[..values.len() - 1].iter().any(|&value| value)
            || (!values[values.len() - 1] && values.len() != order.len())
        {
            return Err(error(INVALID_INPUT));
        }
        return Ok(if values[values.len() - 1] {
            order[values.len() - 1]
        } else {
            order[0]
        });
    }
    Ok(order
        .iter()
        .copied()
        .find(|&index| matches_variant(&request.names[index], request))
        .unwrap_or(order[0]))
}
/// Construct a fixed typed error.
#[must_use]
pub fn error(message: &'static str) -> RustError {
    RustError::Error(message.into())
}

#[cfg(test)]
mod tests;
