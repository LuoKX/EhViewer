//! Recovery field selection and staged-row lifecycle. Room retains transactions and identities.
use crate::{
    contracts::{ApiEnvelope, RustError},
    recovery::{RecoveryPersistenceAction, RecoveryStatus},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::HashSet;

/// Bounded encoded request and response size.
pub const MAX_BYTES: i32 = 16 * 1024 * 1024;
/// Maximum rows in one observation.
pub const MAX_ITEMS: usize = 100_000;

/// Raw Java text, including unpaired surrogates.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    /// UTF-16LE code units.
    pub value: ByteBuf,
}

/// Versioned phase request. Unrelated fields are rejected by the tagged operation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    /// Must equal one.
    pub version: u8,
    /// Pure operation at the original effect boundary.
    #[serde(flatten)]
    pub operation: Operation,
}

/// Independent policies: formal nonblank merge is intentionally distinct from staging null fallback.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Operation {
    /// Choose old fields without serializing tokens, floats or gallery objects.
    Formal {
        /// Whether a formal row exists.
        exists: bool,
        /// JVM isNullOrBlank for title, Japanese title, thumb, posted, uploader and language.
        blank: Vec<bool>,
        /// Existing category.
        category: i32,
        /// Existing rating's raw IEEE bits.
        rating_bits: u32,
        /// Whether old tags are nonempty.
        has_tags: bool,
        /// Existing page count.
        pages: i32,
    },
    /// Stable trim, empty filtering and deduplication of old artists followed by incoming artists.
    Artists {
        /// Names in source order.
        names: Vec<Text>,
        /// Observed JVM whitespace units.
        whitespace: Vec<u16>,
    },
    /// Stage one inspected candidate or remove its redundant old row.
    Stage {
        /// Conflict planner's action.
        action: RecoveryPersistenceAction,
        /// Conflict planner's status.
        status: RecoveryStatus,
        /// Original issues followed by the decision issue, using Room identifiers.
        issues: Vec<String>,
        /// Whether Android could resolve the promotion path.
        path_available: bool,
        /// Non-null incoming gid, token, page token, page index, title and page count.
        present: Vec<bool>,
    },
    /// Preserve nonblank issue fragments verbatim, then append the promotion failure marker.
    Failure {
        /// Original persisted issue string.
        issues_text: ByteBuf,
        /// Observed JVM whitespace units.
        whitespace: Vec<u16>,
    },
    /// Decide whether an exhaustive scan may mark unseen rows missing.
    Finish {
        /// Tree, candidate and inspection limit observations.
        limits: Vec<bool>,
    },
    /// Count pending rows after the platform has finished persistence.
    Pending {
        /// Root equality observations, one per row.
        same_root: Vec<bool>,
        /// `SOURCE_MISSING` observations, one per row.
        missing: Vec<bool>,
    },
}

/// Closed phase result. Empty fields are retained to make validation explicit.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Selected source fields or phase flags.
    pub flags: Vec<bool>,
    /// Input name slot and UTF-16 start/end offsets for each retained artist.
    pub spans: Vec<[usize; 3]>,
    /// Persisted issue string as UTF-16LE, only for Stage and Failure.
    pub text: Option<ByteBuf>,
    /// Closed Room status identifier, only for Stage and Failure.
    pub status: Option<String>,
    /// Pending count, only for Pending.
    pub count: Option<usize>,
}

/// Plans a complete phase without accessing Room or changing objects.
#[must_use]
pub fn build_envelope(r: &Request) -> ApiEnvelope<Plan> {
    match plan(r) {
        Ok(plan) => ApiEnvelope::ok(1, "recoveryPersistence", plan),
        Err(message) => {
            ApiEnvelope::err(1, "recoveryPersistence", RustError::Error(message.into()))
        }
    }
}

fn plan(r: &Request) -> Result<Plan, &'static str> {
    let invalid = "Invalid recovery persistence input";
    if r.version != 1 {
        return Err(invalid);
    }
    let mut p = Plan {
        flags: vec![],
        spans: vec![],
        text: None,
        status: None,
        count: None,
    };
    match &r.operation {
        Operation::Formal {
            exists,
            blank,
            category,
            rating_bits,
            has_tags,
            pages,
        } => {
            if blank.len() != 6 {
                return Err(invalid);
            }
            p.flags = vec![
                *exists,
                !blank[0],
                !blank[1],
                !blank[2],
                *category != 0,
                !blank[3],
                !blank[4],
                f32::from_bits(*rating_bits) != 0.0,
                *has_tags,
                *pages > 0,
                !blank[5],
            ];
            for flag in &mut p.flags {
                *flag &= exists;
            }
        }
        Operation::Artists { names, whitespace } => {
            p.spans = merge_artists(names, whitespace)?;
        }
        Operation::Stage {
            action,
            status,
            issues,
            path_available,
            present,
        } => {
            stage(&mut p, *action, *status, issues, *path_available, present)?;
        }
        Operation::Failure {
            issues_text,
            whitespace,
        } => {
            p.text = Some(failure_issues(issues_text, whitespace)?);
            p.status = Some("PATH_CONFLICT".into());
        }
        Operation::Finish { limits } => {
            if limits.len() != 3 {
                return Err(invalid);
            }
            let limited = limits.iter().any(|b| *b);
            p.flags = vec![limited, !limited];
        }
        Operation::Pending { same_root, missing } => {
            if same_root.len() != missing.len() || same_root.len() > MAX_ITEMS {
                return Err(invalid);
            }
            p.count = Some(
                same_root
                    .iter()
                    .zip(missing)
                    .filter(|(root, missing)| **root && !**missing)
                    .count(),
            );
        }
    }
    Ok(p)
}

fn merge_artists(names: &[Text], whitespace: &[u16]) -> Result<Vec<[usize; 3]>, &'static str> {
    let invalid = "Invalid recovery persistence input";
    let mut spans = Vec::new();
    if names.len() > MAX_ITEMS
        || names.iter().map(|n| n.value.len()).sum::<usize>() > 8 * 1024 * 1024
    {
        return Err(invalid);
    }
    let whitespace = white_set(whitespace)?;
    let mut seen = HashSet::new();
    for (index, name) in names.iter().enumerate() {
        let units = units(&name.value)?;
        let start = units
            .iter()
            .position(|c| !whitespace.contains(c))
            .unwrap_or(units.len());
        let end = units
            .iter()
            .rposition(|c| !whitespace.contains(c))
            .map_or(start, |i| i + 1);
        if start != end && seen.insert(name.value[start * 2..end * 2].to_vec()) {
            spans.push([index, start, end]);
        }
    }
    Ok(spans)
}

fn stage(
    p: &mut Plan,
    action: RecoveryPersistenceAction,
    status: RecoveryStatus,
    issues: &[String],
    path_available: bool,
    present: &[bool],
) -> Result<(), &'static str> {
    let invalid = "Invalid recovery persistence input";
    if present.len() != 6
        || issues.len() > 64
        || issues.iter().any(|s| {
            s.is_empty() || s.len() > 64 || !s.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')
        })
    {
        return Err(invalid);
    }
    if action == RecoveryPersistenceAction::KeepExisting {
        p.flags.push(true);
        return Ok(());
    }
    p.flags.push(false);
    p.flags.extend(present);
    let blocked = action == RecoveryPersistenceAction::PromoteFinished && !path_available;
    let mut merged = Vec::new();
    for issue in issues
        .iter()
        .map(String::as_str)
        .chain(blocked.then_some("ANDROID_PATH_CONFLICT"))
    {
        if !merged.contains(&issue) {
            merged.push(issue);
        }
    }
    p.text = Some(raw(&merged.join(",")));
    p.status = Some(
        if blocked {
            "PATH_CONFLICT"
        } else {
            match status {
                RecoveryStatus::Ready => "READY",
                RecoveryStatus::MissingIdentity => "MISSING_IDENTITY",
                RecoveryStatus::Incomplete => "CONTENT_INCOMPLETE",
                RecoveryStatus::Unreadable => "UNREADABLE",
                RecoveryStatus::LimitExceeded => "LIMIT_EXCEEDED",
                RecoveryStatus::SourceMissing => "SOURCE_MISSING",
                RecoveryStatus::Conflict if merged.contains(&"DUPLICATE_SOURCE") => {
                    "DUPLICATE_SOURCE"
                }
                RecoveryStatus::Conflict if merged.contains(&"EXISTING_MAPPING_CONFLICT") => {
                    "PATH_CONFLICT"
                }
                RecoveryStatus::Conflict => "IDENTITY_CONFLICT",
            }
        }
        .into(),
    );
    Ok(())
}

fn failure_issues(issues_text: &[u8], whitespace: &[u16]) -> Result<ByteBuf, &'static str> {
    let whitespace = white_set(whitespace)?;
    let units = units(issues_text)?;
    let mut seen = HashSet::new();
    let mut merged = Vec::new();
    let marker: Vec<u16> = "PROMOTION_CONFLICT".encode_utf16().collect();
    for part in units
        .split(|&c| c == 44)
        .chain(std::iter::once(marker.as_slice()))
    {
        if part.iter().any(|c| !whitespace.contains(c)) && seen.insert(part.to_vec()) {
            if !merged.is_empty() {
                merged.push(44_u16);
            }
            merged.extend(part);
        }
    }
    Ok(merged
        .iter()
        .flat_map(|u| u.to_le_bytes())
        .collect::<Vec<_>>()
        .into())
}

fn white_set(whitespace: &[u16]) -> Result<HashSet<u16>, &'static str> {
    let set: HashSet<_> = whitespace.iter().copied().collect();
    if whitespace.len() > 65_536 || set.len() != whitespace.len() {
        return Err("Invalid recovery whitespace observations");
    }
    Ok(set)
}

fn units(raw: &[u8]) -> Result<Vec<u16>, &'static str> {
    if !raw.len().is_multiple_of(2) || raw.len() > 1024 * 1024 {
        return Err("Invalid recovery text size");
    }
    Ok(raw
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect())
}

fn raw(s: &str) -> ByteBuf {
    s.encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests;
