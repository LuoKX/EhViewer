//! Pure EH recovery lookup order, candidate selection, reconciliation and failure policy.
//! Android retains HTTP/caching, exception observation, path/Room guards and promotion effects.

use crate::contracts::{ApiEnvelope, RustError};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::LazyLock;

/// V1 EH recovery protocol version.
pub const API_VERSION: u8 = 1;
/// Stable EH recovery envelope discriminator.
pub const MODULE: &str = "recoveryEhLookup";

/// Complete pure operation selected by Android.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LookupKind {
    /// Build identity hints and ordered lookup steps.
    Lookup,
    /// Select a bounded subset of observed search results.
    Select,
    /// Validate selected identity before Android performs hydration HTTP.
    ValidateIdentity,
    /// Reconcile hydrated metadata with local completeness/path facts.
    Apply,
    /// Record a failed/no-match EH query.
    QueryFailure,
    /// Classify a query or platform promotion failure.
    Failure,
}
/// Search-result selection policy.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionKind {
    /// Prefer exact normalized titles, then enforce a known gid.
    Title,
    /// Enforce the known gid.
    Gid,
    /// Validate identities without a title filter.
    Identity,
}
/// Non-sensitive observations for one HTTP result; UI objects remain Android-owned.
#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    /// Gallery ID.
    pub gid: i64,
    /// Public gallery token.
    pub token: String,
    /// Observed display title.
    pub title: Option<String>,
}
/// Android-observed, closed Room promotion failure facts, not arbitrary exception messages.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PromotionSignal {
    /// Candidate was deleted.
    SourceMissing,
    /// Identity/version/state changed.
    IdentityConflict,
    /// Trusted or complete page proof is absent.
    PageCountConflict,
    /// Path changed before promotion.
    PathConflict,
    /// Existing gallery/source mapping conflicts.
    MappingConflict,
    /// An active download prevents replacement.
    ActiveDownloadConflict,
    /// Formal gallery identity conflicts, after higher-priority platform failures.
    FormalIdentityConflict,
    /// Promoted record could not be reloaded.
    LocalPromotionFailed,
}
/// Versioned facts for one pure recovery operation.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupRequest {
    /// Contract version.
    pub version: u8,
    /// Selected operation.
    pub kind: LookupKind,
    /// Stored/inferred known ID or selected gallery ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gid: Option<i64>,
    /// Stored/selected public gallery token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Stored title or title search hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Android display name, not a physical path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    /// Whether Android holds page-token evidence.
    #[serde(default, skip_serializing_if = "is_default")]
    pub has_page_token: bool,
    /// Page-token index, zero based.
    #[serde(default, skip_serializing_if = "is_default")]
    pub page_index: i32,
    /// Result filtering operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SelectionKind>,
    /// Observed search results in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
    /// Existing non-displayable issue CSV.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub issues: String,
    /// Existing evidence CSV.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    /// Locally observed pages.
    #[serde(default, skip_serializing_if = "is_default")]
    pub local_pages: i32,
    /// Hydrated EH page count.
    #[serde(default, skip_serializing_if = "is_default")]
    pub declared_pages: i32,
    /// Previously trusted ID, used to reject identity changes on apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_gid: Option<i64>,
    /// Android proved that a safe promotion path exists.
    #[serde(default, skip_serializing_if = "is_default")]
    pub usable_path: bool,
    /// At most eight exception class names, never messages/URLs/credentials.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cause_types: Vec<String>,
    /// Closed promotion facts collected by Android.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub promotion_signals: Vec<PromotionSignal>,
    /// Latest Room evidence, distinguished from an absent row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_evidence: Option<String>,
    /// Latest Room issues, distinguished from original candidate issues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_issues: Option<String>,
    /// Current persisted status for an already-applied promotion failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_status: Option<String>,
    /// Explicit no-match result, otherwise classify the causal facts.
    #[serde(default, skip_serializing_if = "is_default")]
    pub not_found: bool,
}
/// Ordered HTTP/identity lookup action.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupStep {
    /// pageToken, storedToken, title or gid.
    pub kind: String,
    /// Non-sensitive cache identity; Android alone owns the scheduler.
    pub key: String,
    /// Search text, absent for direct identity steps.
    pub query: Option<String>,
}
/// Complete result; fields are populated only for their echoed operation.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupPlan {
    /// Echoed operation.
    pub kind: LookupKind,
    /// Resolved known ID for lookup.
    pub gid: Option<i64>,
    /// Resolved lookup title.
    pub title: Option<String>,
    /// Ordered actions.
    pub steps: Vec<LookupStep>,
    /// Source indices selected from the Android-owned result list.
    pub selected: Vec<usize>,
    /// Validated persistence status name.
    pub status: Option<String>,
    /// Reconciled issues.
    pub issues: Option<String>,
    /// Reconciled evidence after apply.
    pub evidence: Option<String>,
    /// Whether failure targets the current applied row instead of original query candidate.
    pub promotion_failure: bool,
}

const RESOLVED: [&str; 4] = [
    "MISSING_GID",
    "MISSING_TOKEN",
    "UNKNOWN_PAGE_COUNT",
    "INVALID_METADATA",
];
const LOOKUP_ISSUES: [&str; 9] = [
    "EH_QUERY_FAILED",
    "EH_NO_MATCH",
    "EH_AUTH_REQUIRED",
    "EH_CLOUDFLARE_FAILED",
    "EH_DNS_FAILED",
    "EH_TIMEOUT",
    "EH_NETWORK_FAILED",
    "EH_PARSE_FAILED",
    "EH_RESPONSE_FAILED",
];
static SOURCE_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^([1-9][0-9]{0,18})(?:-([^\n\r\u{0085}\u{2028}\u{2029}]+)|\.(?:cbz|zip|rar|7z|tar|tar\.gz|tar\.xz))\z").expect("source-controlled recovery name")
});

/// Builds a deterministic operation plan with fixed, input-free failures.
#[must_use]
pub fn build_envelope(request: &LookupRequest) -> ApiEnvelope<LookupPlan> {
    if request.version != API_VERSION
        || request.candidates.len() > 1000
        || request.cause_types.len() > 8
        || request.promotion_signals.len() > 8
    {
        return invalid();
    }
    let mut plan = LookupPlan {
        kind: request.kind,
        gid: None,
        title: None,
        steps: Vec::new(),
        selected: Vec::new(),
        status: None,
        issues: None,
        evidence: None,
        promotion_failure: false,
    };
    match request.kind {
        LookupKind::Lookup => lookup(request, &mut plan),
        LookupKind::Select => {
            let Some(selection) = request.selection else {
                return invalid();
            };
            select(request, selection, &mut plan);
        }
        LookupKind::ValidateIdentity | LookupKind::Apply => {
            let Some(gid) = request.gid.filter(|gid| *gid > 0) else {
                return invalid();
            };
            if request.known_gid.is_some_and(|known| known != gid)
                || !request.token.as_deref().is_some_and(valid_token)
            {
                return invalid();
            }
            if request.kind == LookupKind::Apply {
                apply(request, &mut plan);
            }
        }
        LookupKind::QueryFailure => query_failure(request, &mut plan),
        LookupKind::Failure => failure(request, &mut plan),
    }
    ApiEnvelope::ok(API_VERSION, MODULE, plan)
}
fn invalid() -> ApiEnvelope<LookupPlan> {
    ApiEnvelope::err(
        API_VERSION,
        MODULE,
        RustError::Error("Invalid EH recovery input".to_owned()),
    )
}
fn lookup(request: &LookupRequest, plan: &mut LookupPlan) {
    let name = request
        .display_name
        .trim_matches(whitespace)
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let matched = SOURCE_NAME.captures(name);
    let inferred = matched
        .as_ref()
        .and_then(|value| value.get(1))
        .and_then(|value| value.as_str().parse::<i64>().ok())
        .filter(|gid| *gid > 0);
    let gid = request.gid.or(inferred);
    let stored = request
        .title
        .as_deref()
        .unwrap_or_default()
        .trim_matches(whitespace);
    let stripped = gid
        .map(|gid| {
            stored
                .strip_prefix(&format!("{gid}-"))
                .unwrap_or(stored)
                .trim_matches(whitespace)
        })
        .unwrap_or_default();
    let generated = matched
        .as_ref()
        .and_then(|value| value.get(2))
        .map_or("", |value| value.as_str().trim_matches(whitespace));
    let title = if !stripped.is_empty() {
        stripped
    } else if !generated.is_empty() {
        generated
    } else {
        stored
    };
    plan.gid = gid;
    plan.title = Some(title.to_owned());
    if let Some(gid) = gid {
        if request.has_page_token {
            plan.steps.push(step(
                "pageToken",
                format!("gtoken:{gid}:{}", request.page_index),
                None,
            ));
        }
        if request.token.as_deref().is_some_and(valid_token) {
            plan.steps.push(step("storedToken", String::new(), None));
        }
    }
    if !title.is_empty() {
        plan.steps.push(step(
            "title",
            format!("search:title:{}", normalize(title)),
            Some(format!(
                "\"{}\"",
                title.replace('"', " ").trim_matches(whitespace)
            )),
        ));
    }
    if let Some(gid) = gid {
        plan.steps.push(step(
            "gid",
            format!("search:gid:{gid}"),
            Some(format!("gid:{gid}")),
        ));
    }
}
fn step(kind: &str, key: String, query: Option<String>) -> LookupStep {
    LookupStep {
        kind: kind.to_owned(),
        key,
        query,
    }
}
fn select(request: &LookupRequest, selection: SelectionKind, plan: &mut LookupPlan) {
    let title = normalize(request.title.as_deref().unwrap_or_default());
    let exact_exists = selection == SelectionKind::Title
        && request
            .candidates
            .iter()
            .any(|item| normalize(item.title.as_deref().unwrap_or_default()) == title);
    let mut seen = HashSet::new();
    for (index, item) in request.candidates.iter().enumerate() {
        if exact_exists && normalize(item.title.as_deref().unwrap_or_default()) != title {
            continue;
        }
        if selection != SelectionKind::Identity && request.gid.is_some_and(|gid| item.gid != gid) {
            continue;
        }
        if item.gid <= 0
            || !valid_token(&item.token)
            || !seen.insert((item.gid, item.token.as_str()))
        {
            continue;
        }
        plan.selected.push(index);
        if plan.selected.len() == 25 {
            break;
        }
    }
}
fn apply(request: &LookupRequest, plan: &mut LookupPlan) {
    let mut issues: Vec<String> = csv(&request.issues)
        .into_iter()
        .filter(|issue| {
            !RESOLVED.contains(&issue.as_str()) && !LOOKUP_ISSUES.contains(&issue.as_str())
        })
        .collect();
    let complete = issues.is_empty()
        && request.local_pages > 0
        && request.declared_pages == request.local_pages;
    plan.status = Some(
        if !complete {
            "CONTENT_INCOMPLETE"
        } else if !request.usable_path {
            "PATH_CONFLICT"
        } else {
            "READY"
        }
        .to_owned(),
    );
    if request.declared_pages != request.local_pages {
        issues.push("PAGE_COUNT_CONFLICT".to_owned());
    }
    if !request.usable_path {
        issues.push("ANDROID_PATH_CONFLICT".to_owned());
    }
    plan.issues = Some(join_unique(issues));
    let mut evidence = csv(&request.evidence);
    evidence.push("EH_LOOKUP".to_owned());
    plan.evidence = Some(join_unique(evidence));
}
fn query_failure(request: &LookupRequest, plan: &mut LookupPlan) {
    plan.status = Some("QUERY_FAILED".to_owned());
    let mut issues: Vec<String> = csv(&request.issues)
        .into_iter()
        .filter(|issue| !LOOKUP_ISSUES.contains(&issue.as_str()))
        .collect();
    issues.push(
        if request.not_found {
            "EH_NO_MATCH"
        } else {
            failure_issue(&request.cause_types)
        }
        .to_owned(),
    );
    plan.issues = Some(join_unique(issues));
}
fn failure(request: &LookupRequest, plan: &mut LookupPlan) {
    let promotion = promotion_failure(&request.promotion_signals);
    let applied = request
        .latest_evidence
        .as_deref()
        .is_some_and(|evidence| csv(evidence).iter().any(|value| value == "EH_LOOKUP"));
    if promotion.is_none() && !applied {
        query_failure(request, plan);
        return;
    }
    plan.promotion_failure = true;
    let (status, issue) = promotion.unwrap_or_else(|| {
        let current = request.current_status.as_deref().unwrap_or("PATH_CONFLICT");
        (
            if current == "READY" {
                "PATH_CONFLICT"
            } else {
                current
            },
            "LOCAL_PROMOTION_FAILED",
        )
    });
    plan.status = Some(status.to_owned());
    let mut issues: Vec<String> = csv(request.latest_issues.as_deref().unwrap_or(&request.issues))
        .into_iter()
        .filter(|issue| !LOOKUP_ISSUES.contains(&issue.as_str()))
        .collect();
    issues.push(issue.to_owned());
    plan.issues = Some(join_unique(issues));
}
fn promotion_failure(signals: &[PromotionSignal]) -> Option<(&'static str, &'static str)> {
    for (signal, status, issue) in [
        (
            PromotionSignal::SourceMissing,
            "SOURCE_MISSING",
            "RECOVERY_SOURCE_MISSING",
        ),
        (
            PromotionSignal::IdentityConflict,
            "IDENTITY_CONFLICT",
            "IDENTITY_CONFLICT",
        ),
        (
            PromotionSignal::PageCountConflict,
            "CONTENT_INCOMPLETE",
            "PAGE_COUNT_CONFLICT",
        ),
        (
            PromotionSignal::PathConflict,
            "PATH_CONFLICT",
            "ANDROID_PATH_CONFLICT",
        ),
        (
            PromotionSignal::MappingConflict,
            "PATH_CONFLICT",
            "EXISTING_MAPPING_CONFLICT",
        ),
        (
            PromotionSignal::ActiveDownloadConflict,
            "IDENTITY_CONFLICT",
            "ACTIVE_DOWNLOAD_CONFLICT",
        ),
        (
            PromotionSignal::FormalIdentityConflict,
            "IDENTITY_CONFLICT",
            "IDENTITY_CONFLICT",
        ),
        (
            PromotionSignal::LocalPromotionFailed,
            "PATH_CONFLICT",
            "LOCAL_PROMOTION_FAILED",
        ),
    ] {
        if signals.contains(&signal) {
            return Some((status, issue));
        }
    }
    None
}
fn failure_issue(types: &[String]) -> &'static str {
    for (names, issue) in [
        (&["NotLoggedInException"][..], "EH_AUTH_REQUIRED"),
        (&["CloudflareBypassException"][..], "EH_CLOUDFLARE_FAILED"),
        (&["UnknownHostException"][..], "EH_DNS_FAILED"),
        (
            &["SocketTimeoutException", "HttpTimeoutException"][..],
            "EH_TIMEOUT",
        ),
        (
            &["ConnectException", "IOException"][..],
            "EH_NETWORK_FAILED",
        ),
        (
            &[
                "ParseException",
                "SerializationException",
                "RustFailureException",
            ][..],
            "EH_PARSE_FAILED",
        ),
        (
            &[
                "EhException",
                "IllegalArgumentException",
                "IllegalStateException",
                "NoSuchElementException",
            ][..],
            "EH_RESPONSE_FAILED",
        ),
    ] {
        if types.iter().any(|value| names.contains(&value.as_str())) {
            return issue;
        }
    }
    "EH_QUERY_FAILED"
}
fn valid_token(token: &str) -> bool {
    token.len() == 10
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn normalize(title: &str) -> String {
    title
        .to_lowercase()
        .split(|value| matches!(value, ' ' | '\t'..='\r'))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(whitespace)
        .to_owned()
}
fn whitespace(value: char) -> bool {
    (value.is_whitespace() && value != '\u{0085}') || matches!(value, '\u{001c}'..='\u{001f}')
}
fn csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .filter(|value| !value.chars().all(whitespace))
        .map(str::to_owned)
        .collect()
}
fn join_unique(values: Vec<String>) -> String {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>()
        .join(",")
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

#[cfg(test)]
mod tests;
