//! Pure download-directory recovery contracts and deterministic planning.
//!
//! Android owns traversal, SAF handles, Room transactions, `WorkManager`, and all
//! network access. This module receives bounded bytes and entry facts, reconciles
//! local evidence, proves page completeness, and returns one persistence intent.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use sha2::{Digest, Sha256};

pub use crate::comic_info::MAX_COMIC_INFO_BYTES;
use crate::comic_info::decode_xml as decode_comic_info_xml;
use crate::contracts::{ApiEnvelope, RustError};

/// Version of the download-recovery CBOR contract.
pub const DOWNLOAD_RECOVERY_API_VERSION: u8 = 1;
/// Stable module identifier for download-recovery envelopes.
pub const DOWNLOAD_RECOVERY_MODULE: &str = "downloadRecovery";
/// Maximum accepted `.ehviewer` metadata length.
pub const MAX_SPIDER_INFO_BYTES: usize = crate::spider_info::MAX_BYTES;
/// Maximum number of local entry facts accepted for one candidate.
pub const MAX_LOCAL_ENTRIES: usize = 100_000;

use crate::local_pages::IMAGE_EXTENSIONS;

/// Physical kind of a locally discovered recovery candidate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoverySourceKind {
    /// A directory containing loose pages and optional sidecar metadata.
    Directory,
    /// A supported archive opened through the Rust archive facade.
    Archive,
}

/// Stable, opaque inputs used to identify one source across rescans.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySourceKeyInput {
    /// Stable identifier of the configured scan root, never a display path.
    pub root_id: String,
    /// Provider document ID, durable URI, or root-relative physical identity.
    pub source_id: String,
    /// Physical candidate kind.
    pub source_kind: RecoverySourceKind,
}

/// Stable caller-observed facts used to skip unchanged candidates safely.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryFingerprintInput {
    /// Total source byte size when known.
    pub size: Option<u64>,
    /// Platform modification timestamp in milliseconds when known.
    pub modified_at_millis: Option<i64>,
    /// Optional caller-computed content digest bytes.
    #[serde(default)]
    pub content_digest: Option<ByteBuf>,
}

/// One bounded directory or archive entry observation supplied by an Android adapter.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntryInput {
    /// Exact entry name relative to its candidate container.
    pub name: String,
    /// Declared or observed uncompressed byte size.
    pub size: u64,
    /// Whether the entry is a regular file.
    pub regular: bool,
    /// Whether the codec reported encryption.
    #[serde(default)]
    pub encrypted: bool,
    /// Whether the complete entry was read and its integrity check passed.
    #[serde(default)]
    pub readable: bool,
}

/// Archive facts collected by the recovery-specific Rust archive inspector.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveInput {
    /// Stable content-detected format name.
    pub format: String,
    /// Whether archive enumeration completed inside all budgets.
    pub complete: bool,
    /// Closed archive issues observed by the Android descriptor adapter.
    #[serde(default)]
    pub issues: Vec<RecoveryIssue>,
}

/// Versioned request for one local recovery candidate.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCandidateRequest {
    /// Requested contract version.
    pub version: u8,
    /// Stable source identity inputs.
    pub source: RecoverySourceKeyInput,
    /// Stable change-detection inputs.
    pub fingerprint: RecoveryFingerprintInput,
    /// Non-sensitive display label owned by Android UI.
    pub display_name: String,
    /// Optional bounded `.ehviewer` bytes.
    #[serde(default)]
    pub spider_info: Option<ByteBuf>,
    /// Optional bounded root `ComicInfo.xml` bytes.
    #[serde(default)]
    pub comic_info: Option<ByteBuf>,
    /// Bounded entry metadata; image bodies never cross this boundary.
    #[serde(default)]
    pub entries: Vec<RecoveryEntryInput>,
    /// Archive-only inspection summary.
    #[serde(default)]
    pub archive: Option<RecoveryArchiveInput>,
}

/// Provenance of one local recovery fact.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryEvidenceKind {
    /// Current or compatible legacy `.ehviewer` metadata.
    SpiderInfo,
    /// Root `ComicInfo.xml` metadata.
    ComicInfo,
    /// Strict one-based loose image entries.
    LooseImages,
    /// Recovery-specific archive inspection.
    Archive,
}

/// One normalized evidence record retained for persistence and review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEvidence {
    /// Evidence provenance.
    pub kind: RecoveryEvidenceKind,
    /// Positive remote gallery identifier when present.
    pub gid: Option<i64>,
    /// Gallery token when present. Consumers must never render or log this field.
    pub token: Option<String>,
    /// Local title when present.
    pub title: Option<String>,
    /// Trusted declared page count when present.
    pub declared_pages: Option<u32>,
    /// Number of strict image entries observed from this source.
    pub observed_pages: Option<u32>,
}

/// Stable, non-sensitive reason why a candidate cannot be promoted directly.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryIssue {
    /// No valid positive gallery identifier is available.
    MissingGid,
    /// No valid gallery token is available.
    MissingToken,
    /// A structured metadata source is malformed or outside its byte budget.
    InvalidMetadata,
    /// Structured identity sources disagree.
    IdentityConflict,
    /// Trusted page-count sources disagree.
    PageCountConflict,
    /// No trusted positive page count is available.
    UnknownPageCount,
    /// At least one page from `1..=N` is absent.
    MissingPage,
    /// More than one valid file maps to one page ordinal.
    DuplicatePage,
    /// A temporary `.tmp` file remains in the candidate.
    TemporaryFile,
    /// An expected image is empty or non-regular.
    InvalidImage,
    /// The archive is encrypted and recovery intentionally accepts no password.
    EncryptedArchive,
    /// The archive could not be completely read or integrity checked.
    UnreadableArchive,
    /// The archive contains an unsafe absolute or parent-traversal entry path.
    UnsafeArchivePath,
    /// A configured entry, byte, depth, or time budget was exhausted.
    LimitExceeded,
    /// Another source competes for the same gallery identity.
    DuplicateSource,
    /// A currently valid database source mapping must be preserved.
    ExistingMappingConflict,
    /// A previously staged source was not found in the current scan.
    SourceMissing,
}

/// Stable recovery state persisted by the Android Room adapter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryStatus {
    /// Identity and local content are complete and eligible for promotion.
    Ready,
    /// One or both remote identity fields are absent.
    MissingIdentity,
    /// Local content cannot be proved complete.
    Incomplete,
    /// Identity or source ownership is ambiguous.
    Conflict,
    /// The archive is encrypted, corrupt, unsupported, or unreadable.
    Unreadable,
    /// A resource limit stopped inspection.
    LimitExceeded,
    /// A previously known source is currently absent.
    SourceMissing,
}

/// Persistence intent executed atomically by Android Room.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryPersistenceAction {
    /// Promote into the formal gallery/download tables as finished.
    PromoteFinished,
    /// Preserve an already complete mapping and discard redundant staging state.
    KeepExisting,
    /// Upsert into the isolated recovery-candidate table.
    Stage,
}

/// Complete deterministic candidate result returned to Android.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCandidatePlan {
    /// SHA-256 source identity derived from length-prefixed opaque inputs.
    pub source_key: String,
    /// SHA-256 change fingerprint derived from typed caller facts.
    pub fingerprint: String,
    /// Candidate source kind.
    pub source_kind: RecoverySourceKind,
    /// Android-provided UI label.
    pub display_name: String,
    /// Reconciled positive remote gallery identifier.
    pub gid: Option<i64>,
    /// Reconciled gallery token; never include it in diagnostics.
    pub token: Option<String>,
    /// Best local title without remote enrichment.
    pub title: Option<String>,
    /// Reconciled trusted page count.
    pub declared_pages: Option<u32>,
    /// Strict valid local image count.
    pub local_page_count: u32,
    /// Normalized evidence in deterministic provenance order.
    pub evidence: Vec<RecoveryEvidence>,
    /// Deduplicated issues in stable enum order.
    pub issues: Vec<RecoveryIssue>,
    /// Candidate state derived from all blocking issues.
    pub status: RecoveryStatus,
    /// Android persistence action selected before any transaction.
    pub action: RecoveryPersistenceAction,
}

/// Parses bounded current-CBOR or legacy-v2 `.ehviewer` metadata.
///
/// # Errors
///
/// Returns a fixed, token-free error for oversized, malformed, or invalid metadata.
pub fn parse_spider_info(bytes: &[u8]) -> Result<RecoveryEvidence, RustError> {
    let (gid, token, pages) =
        crate::spider_info::decode_identity(bytes).map_err(|_| invalid_metadata())?;
    if gid <= 0 || !valid_token(&token) || pages <= 0 {
        return Err(invalid_metadata());
    }
    Ok(RecoveryEvidence {
        kind: RecoveryEvidenceKind::SpiderInfo,
        gid: Some(gid),
        token: Some(token),
        title: None,
        declared_pages: u32::try_from(pages).ok(),
        observed_pages: None,
    })
}

/// Parses bounded root `ComicInfo.xml` metadata and an optional strict E/EX URL.
///
/// # Errors
///
/// Returns a fixed, source-free error for malformed or oversized XML.
pub fn parse_comic_info(bytes: &[u8]) -> Result<RecoveryEvidence, RustError> {
    let info = decode_comic_info_xml(bytes).map_err(|_| invalid_metadata())?;
    let (gid, token) = info.web.as_deref().and_then(parse_gallery_url).unzip();
    Ok(RecoveryEvidence {
        kind: RecoveryEvidenceKind::ComicInfo,
        gid,
        token,
        title: info.series,
        declared_pages: u32::try_from(info.page_count)
            .ok()
            .filter(|pages| *pages > 0),
        observed_pages: None,
    })
}

/// Reconciles bounded metadata and strict entry facts into one persistence plan.
///
/// # Errors
///
/// Returns a fixed error only for request-level contract or resource-limit failure.
pub fn inspect_recovery_candidate(
    request: RecoveryCandidateRequest,
) -> Result<RecoveryCandidatePlan, RustError> {
    validate_candidate_request(&request)?;
    let source_key = source_key(&request.source);
    let fingerprint = fingerprint(&request.fingerprint);
    let mut evidence = Vec::new();
    let mut issues = BTreeSet::new();
    if let Some(bytes) = request.spider_info.as_deref() {
        match parse_spider_info(bytes) {
            Ok(value) => evidence.push(value),
            Err(_) => {
                issues.insert(RecoveryIssue::InvalidMetadata);
            }
        }
    }
    if let Some(bytes) = request.comic_info.as_deref() {
        match parse_comic_info(bytes) {
            Ok(value) => evidence.push(value),
            Err(_) => {
                issues.insert(RecoveryIssue::InvalidMetadata);
            }
        }
    }

    let page_summary = summarize_candidate_pages(&request);
    issues.extend(page_summary.issues.iter().copied());
    evidence.push(RecoveryEvidence {
        kind: if request.source.source_kind == RecoverySourceKind::Archive {
            RecoveryEvidenceKind::Archive
        } else {
            RecoveryEvidenceKind::LooseImages
        },
        gid: None,
        token: None,
        title: None,
        declared_pages: None,
        observed_pages: Some(page_summary.valid_count),
    });
    if let Some(archive) = request.archive.as_ref() {
        issues.extend(archive.issues.iter().copied());
        if !archive.complete {
            issues.insert(RecoveryIssue::UnreadableArchive);
        }
    }

    let gid = reconcile(&evidence, |item| item.gid, &mut issues);
    let token = reconcile_owned(&evidence, |item| item.token.as_ref(), &mut issues);
    let declared_pages = reconcile_page_counts(&evidence, |item| item.declared_pages, &mut issues);
    let title = evidence.iter().find_map(|item| item.title.clone());
    if gid.is_none() {
        issues.insert(RecoveryIssue::MissingGid);
    }
    if token.is_none() {
        issues.insert(RecoveryIssue::MissingToken);
    }
    match declared_pages {
        None => {
            issues.insert(RecoveryIssue::UnknownPageCount);
        }
        Some(expected) if !page_summary.is_exact(expected) => {
            issues.insert(RecoveryIssue::MissingPage);
        }
        Some(_) => {}
    }

    let issues = issues.into_iter().collect::<Vec<_>>();
    let status = status_for(&issues);
    let action = if status == RecoveryStatus::Ready {
        RecoveryPersistenceAction::PromoteFinished
    } else {
        RecoveryPersistenceAction::Stage
    };
    Ok(RecoveryCandidatePlan {
        source_key,
        fingerprint,
        source_kind: request.source.source_kind,
        display_name: request.display_name,
        gid,
        token,
        title,
        declared_pages,
        local_page_count: page_summary.valid_count,
        evidence,
        issues,
        status,
        action,
    })
}

fn validate_candidate_request(request: &RecoveryCandidateRequest) -> Result<(), RustError> {
    let invalid_archive = request.archive.as_ref().is_some_and(|archive| {
        archive.format.is_empty()
            || archive.issues.iter().any(|issue| {
                !matches!(
                    issue,
                    RecoveryIssue::EncryptedArchive
                        | RecoveryIssue::UnsafeArchivePath
                        | RecoveryIssue::LimitExceeded
                )
            })
    });
    if request.version != DOWNLOAD_RECOVERY_API_VERSION
        || request.source.root_id.is_empty()
        || request.source.source_id.is_empty()
        || request.entries.len() > MAX_LOCAL_ENTRIES
        || invalid_archive
        || matches!(
            (request.source.source_kind, request.archive.is_some()),
            (RecoverySourceKind::Directory, true) | (RecoverySourceKind::Archive, false)
        )
    {
        return Err(RustError::Error(
            "invalid download recovery candidate request".to_owned(),
        ));
    }
    Ok(())
}

/// Builds a versioned success or sanitized error envelope for one candidate.
#[must_use]
pub fn inspect_recovery_candidate_envelope(
    request: RecoveryCandidateRequest,
) -> ApiEnvelope<RecoveryCandidatePlan> {
    match inspect_recovery_candidate(request) {
        Ok(plan) => ApiEnvelope::ok(
            DOWNLOAD_RECOVERY_API_VERSION,
            DOWNLOAD_RECOVERY_MODULE,
            plan,
        ),
        Err(error) => ApiEnvelope::err(
            DOWNLOAD_RECOVERY_API_VERSION,
            DOWNLOAD_RECOVERY_MODULE,
            error,
        ),
    }
}

/// Existing formal download mapping used by deterministic conflict planning.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingRecoveryMapping {
    /// Positive remote gallery identifier.
    pub gid: i64,
    /// Stable source key already mapped by Android.
    pub source_key: String,
    /// Whether the mapped source still satisfies completeness checks.
    pub complete: bool,
}

/// Versioned conflict-planning request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryConflictRequest {
    /// Requested protocol version.
    pub version: u8,
    /// Candidate plans from the current scan.
    pub candidates: Vec<RecoveryCandidatePlan>,
    /// Latest Room mappings read immediately before transaction planning.
    pub existing: Vec<ExistingRecoveryMapping>,
}

/// One final action keyed by stable source identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryConflictDecision {
    /// Stable source key.
    pub source_key: String,
    /// Final status after cross-candidate reconciliation.
    pub status: RecoveryStatus,
    /// Final persistence action.
    pub action: RecoveryPersistenceAction,
    /// Additional conflict issue when one was introduced.
    pub issue: Option<RecoveryIssue>,
}

/// Deterministic, source-key-sorted conflict plan.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryConflictPlan {
    /// One decision for every input candidate, sorted by source key.
    pub decisions: Vec<RecoveryConflictDecision>,
}

/// Plans same-GID and existing-mapping conflicts independent of scan order.
///
/// # Errors
///
/// Returns a fixed error for unsupported versions, duplicate source keys, or
/// invalid existing identifiers.
pub fn plan_recovery_conflicts(
    mut request: RecoveryConflictRequest,
) -> Result<RecoveryConflictPlan, RustError> {
    if request.version != DOWNLOAD_RECOVERY_API_VERSION
        || request.existing.iter().any(|mapping| mapping.gid <= 0)
    {
        return Err(RustError::Error(
            "invalid download recovery conflict request".to_owned(),
        ));
    }
    request
        .candidates
        .sort_by(|a, b| a.source_key.cmp(&b.source_key));
    if request
        .candidates
        .windows(2)
        .any(|items| items[0].source_key == items[1].source_key)
    {
        return Err(RustError::Error(
            "duplicate download recovery source key".to_owned(),
        ));
    }
    let existing = request
        .existing
        .into_iter()
        .filter(|mapping| mapping.complete)
        .map(|mapping| (mapping.gid, mapping.source_key))
        .collect::<BTreeMap<_, _>>();
    let mut by_gid = BTreeMap::<i64, Vec<String>>::new();
    for candidate in &request.candidates {
        if candidate.status == RecoveryStatus::Ready
            && let Some(gid) = candidate.gid
        {
            by_gid
                .entry(gid)
                .or_default()
                .push(candidate.source_key.clone());
        }
    }
    let decisions = request
        .candidates
        .into_iter()
        .map(|candidate| {
            let mut status = candidate.status;
            let mut action = candidate.action;
            let mut issue = None;
            if status == RecoveryStatus::Ready
                && let Some(gid) = candidate.gid
            {
                match existing.get(&gid) {
                    Some(mapped) if mapped == &candidate.source_key => {
                        action = RecoveryPersistenceAction::KeepExisting;
                    }
                    Some(_) => {
                        status = RecoveryStatus::Conflict;
                        action = RecoveryPersistenceAction::Stage;
                        issue = Some(RecoveryIssue::ExistingMappingConflict);
                    }
                    None if by_gid.get(&gid).is_some_and(|sources| sources.len() > 1) => {
                        status = RecoveryStatus::Conflict;
                        action = RecoveryPersistenceAction::Stage;
                        issue = Some(RecoveryIssue::DuplicateSource);
                    }
                    _ => {}
                }
            }
            RecoveryConflictDecision {
                source_key: candidate.source_key,
                status,
                action,
                issue,
            }
        })
        .collect();
    Ok(RecoveryConflictPlan { decisions })
}

/// Builds a versioned success or sanitized error envelope for conflict planning.
#[must_use]
pub fn plan_recovery_conflicts_envelope(
    request: RecoveryConflictRequest,
) -> ApiEnvelope<RecoveryConflictPlan> {
    match plan_recovery_conflicts(request) {
        Ok(plan) => ApiEnvelope::ok(
            DOWNLOAD_RECOVERY_API_VERSION,
            DOWNLOAD_RECOVERY_MODULE,
            plan,
        ),
        Err(error) => ApiEnvelope::err(
            DOWNLOAD_RECOVERY_API_VERSION,
            DOWNLOAD_RECOVERY_MODULE,
            error,
        ),
    }
}

/// Returns whether two typed fingerprint inputs describe the same source revision.
#[must_use]
pub fn fingerprint_matches(
    left: &RecoveryFingerprintInput,
    right: &RecoveryFingerprintInput,
) -> bool {
    fingerprint(left) == fingerprint(right)
}

#[derive(Default)]
struct PageSummary {
    valid_count: u32,
    ordinals: BTreeMap<u32, u32>,
    requires_exact_ordinals: bool,
    issues: BTreeSet<RecoveryIssue>,
}

impl PageSummary {
    fn is_exact(&self, expected: u32) -> bool {
        self.valid_count == expected
            && (!self.requires_exact_ordinals
                || (self.ordinals.len() == usize::try_from(expected).unwrap_or(usize::MAX)
                    && (1..=expected).all(|page| self.ordinals.get(&page) == Some(&1))))
            && !self.issues.iter().any(|issue| {
                matches!(
                    issue,
                    RecoveryIssue::DuplicatePage
                        | RecoveryIssue::TemporaryFile
                        | RecoveryIssue::InvalidImage
                )
            })
    }
}

fn summarize_candidate_pages(request: &RecoveryCandidateRequest) -> PageSummary {
    let generated_cbz = request
        .archive
        .as_ref()
        .is_some_and(|archive| archive.format == "generatedCbz");
    if request.source.source_kind == RecoverySourceKind::Directory || generated_cbz {
        summarize_numbered_pages(&request.entries)
    } else {
        summarize_archive_images(&request.entries)
    }
}

fn summarize_numbered_pages(entries: &[RecoveryEntryInput]) -> PageSummary {
    let mut summary = PageSummary {
        requires_exact_ordinals: true,
        ..PageSummary::default()
    };
    for entry in entries {
        if temporary_file(&entry.name) {
            summary.issues.insert(RecoveryIssue::TemporaryFile);
            continue;
        }
        let Some(page) = strict_page_number(&entry.name) else {
            continue;
        };
        if !entry.regular || entry.size == 0 || !entry.readable || entry.encrypted {
            summary.issues.insert(RecoveryIssue::InvalidImage);
            continue;
        }
        summary.valid_count = summary.valid_count.saturating_add(1);
        let count = summary.ordinals.entry(page).or_default();
        *count = count.saturating_add(1);
        if *count > 1 {
            summary.issues.insert(RecoveryIssue::DuplicatePage);
        }
    }
    summary
}

fn summarize_archive_images(entries: &[RecoveryEntryInput]) -> PageSummary {
    let mut summary = PageSummary::default();
    for entry in entries {
        if temporary_file(&entry.name) {
            summary.issues.insert(RecoveryIssue::TemporaryFile);
            continue;
        }
        if playable_image_extension(&entry.name).is_none() {
            continue;
        }
        if !entry.regular || entry.size == 0 || !entry.readable || entry.encrypted {
            summary.issues.insert(RecoveryIssue::InvalidImage);
            continue;
        }
        summary.valid_count = summary.valid_count.saturating_add(1);
    }
    summary
}

fn strict_page_number(name: &str) -> Option<u32> {
    crate::local_pages::page_number(name, false, false)
}

fn playable_image_extension(name: &str) -> Option<&str> {
    let extension = name.rsplit_once('.')?.1;
    IMAGE_EXTENSIONS.contains(&extension).then_some(extension)
}

fn temporary_file(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
}

fn parse_gallery_url(value: &str) -> Option<(i64, String)> {
    let rest = [
        "https://e-hentai.org/g/",
        "https://exhentai.org/g/",
        "https://e-hentai.org/mpv/",
        "https://exhentai.org/mpv/",
    ]
    .into_iter()
    .find_map(|prefix| value.strip_prefix(prefix))?;
    let mut segments = rest.split('/');
    let gid = segments
        .next()?
        .parse::<i64>()
        .ok()
        .filter(|gid| *gid > 0)?;
    let token = segments.next()?;
    if !valid_token(token) {
        return None;
    }
    Some((gid, token.to_owned()))
}

fn valid_token(token: &str) -> bool {
    token.len() == 10
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn reconcile<T: Copy + Eq + Ord>(
    evidence: &[RecoveryEvidence],
    field: impl Fn(&RecoveryEvidence) -> Option<T>,
    issues: &mut BTreeSet<RecoveryIssue>,
) -> Option<T> {
    let values = evidence.iter().filter_map(field).collect::<BTreeSet<_>>();
    if values.len() > 1 {
        issues.insert(RecoveryIssue::IdentityConflict);
        None
    } else {
        values.into_iter().next()
    }
}

fn reconcile_owned(
    evidence: &[RecoveryEvidence],
    field: impl Fn(&RecoveryEvidence) -> Option<&String>,
    issues: &mut BTreeSet<RecoveryIssue>,
) -> Option<String> {
    let values = evidence
        .iter()
        .filter_map(field)
        .cloned()
        .collect::<BTreeSet<_>>();
    if values.len() > 1 {
        issues.insert(RecoveryIssue::IdentityConflict);
        None
    } else {
        values.into_iter().next()
    }
}

fn reconcile_page_counts(
    evidence: &[RecoveryEvidence],
    field: impl Fn(&RecoveryEvidence) -> Option<u32>,
    issues: &mut BTreeSet<RecoveryIssue>,
) -> Option<u32> {
    let values = evidence.iter().filter_map(field).collect::<BTreeSet<_>>();
    if values.len() > 1 {
        issues.insert(RecoveryIssue::PageCountConflict);
        None
    } else {
        values.into_iter().next()
    }
}

fn source_key(input: &RecoverySourceKeyInput) -> String {
    digest_parts(&[
        b"source-v1",
        match input.source_kind {
            RecoverySourceKind::Directory => b"directory",
            RecoverySourceKind::Archive => b"archive",
        },
        input.root_id.as_bytes(),
        input.source_id.as_bytes(),
    ])
}

fn fingerprint(input: &RecoveryFingerprintInput) -> String {
    let size = input.size.map(u64::to_be_bytes);
    let modified = input.modified_at_millis.map(i64::to_be_bytes);
    digest_parts(&[
        b"fingerprint-v1",
        size.as_ref().map_or(&[][..], <[u8; 8]>::as_slice),
        modified.as_ref().map_or(&[][..], <[u8; 8]>::as_slice),
        input
            .content_digest
            .as_ref()
            .map_or(&[][..], |digest| digest.as_slice()),
    ])
}

fn digest_parts(parts: &[&[u8]]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_be_bytes());
        digest.update(part);
    }
    let mut output = String::with_capacity(64);
    for byte in digest.finalize() {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn status_for(issues: &[RecoveryIssue]) -> RecoveryStatus {
    if issues.contains(&RecoveryIssue::SourceMissing) {
        RecoveryStatus::SourceMissing
    } else if issues.contains(&RecoveryIssue::LimitExceeded) {
        RecoveryStatus::LimitExceeded
    } else if issues.iter().any(|issue| {
        matches!(
            issue,
            RecoveryIssue::EncryptedArchive
                | RecoveryIssue::UnreadableArchive
                | RecoveryIssue::UnsafeArchivePath
        )
    }) {
        RecoveryStatus::Unreadable
    } else if issues.iter().any(|issue| {
        matches!(
            issue,
            RecoveryIssue::IdentityConflict
                | RecoveryIssue::PageCountConflict
                | RecoveryIssue::DuplicateSource
                | RecoveryIssue::ExistingMappingConflict
        )
    }) {
        RecoveryStatus::Conflict
    } else if issues.contains(&RecoveryIssue::MissingGid)
        || issues.contains(&RecoveryIssue::MissingToken)
    {
        RecoveryStatus::MissingIdentity
    } else if issues.is_empty() {
        RecoveryStatus::Ready
    } else {
        RecoveryStatus::Incomplete
    }
}

fn invalid_metadata() -> RustError {
    RustError::Error("invalid local recovery metadata".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_kotlin_byte_string_request_golden() {
        let request: RecoveryCandidateRequest =
            serde_cbor::from_slice(&decode_hex(KOTLIN_BYTE_STRING_REQUEST_CBOR_HEX)).unwrap();

        let bytes = ByteBuf::from(vec![0, 128, 255]);
        assert_eq!(request.fingerprint.content_digest, Some(bytes.clone()));
        assert_eq!(request.spider_info, Some(bytes.clone()));
        assert_eq!(request.comic_info, Some(bytes));
        assert!(request.entries.is_empty());
        assert_eq!(request.archive, None);
    }

    #[test]
    fn verified_archive_pages_are_complete_by_declared_count() {
        let entries = vec![entry("chapter/page-a.jpg"), entry("page-b.png")];

        let summary = summarize_archive_images(&entries);

        assert!(summary.is_exact(2));
        assert!(!summary.is_exact(1));
    }

    #[test]
    fn loose_pages_still_require_contiguous_one_based_names() {
        let entries = vec![entry("00000001.jpg"), entry("00000003.jpg")];

        let summary = summarize_numbered_pages(&entries);

        assert!(!summary.is_exact(2));
    }

    #[test]
    fn conflict_planner_keeps_only_the_same_complete_mapping() {
        let source_key = "source-a".to_owned();
        let same = plan_recovery_conflicts(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![ready_candidate(&source_key, 7)],
            existing: vec![ExistingRecoveryMapping {
                gid: 7,
                source_key: source_key.clone(),
                complete: true,
            }],
        })
        .unwrap();
        assert_eq!(
            same.decisions[0].action,
            RecoveryPersistenceAction::KeepExisting
        );
        assert_eq!(same.decisions[0].status, RecoveryStatus::Ready);
        assert_eq!(same.decisions[0].issue, None);

        let conflict = plan_recovery_conflicts(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![ready_candidate(&source_key, 7)],
            existing: vec![ExistingRecoveryMapping {
                gid: 7,
                source_key: "other-source".to_owned(),
                complete: true,
            }],
        })
        .unwrap();
        assert_eq!(
            conflict.decisions[0].action,
            RecoveryPersistenceAction::Stage
        );
        assert_eq!(conflict.decisions[0].status, RecoveryStatus::Conflict);
        assert_eq!(
            conflict.decisions[0].issue,
            Some(RecoveryIssue::ExistingMappingConflict)
        );
    }

    #[test]
    fn parses_current_legacy_comic_info_and_rejects_invalid_metadata() {
        let current = parse_spider_info(&spider_bytes(7, "0123456789", 2)).unwrap();
        assert_eq!(current.gid, Some(7));
        assert_eq!(current.token.as_deref(), Some("0123456789"));
        assert_eq!(current.declared_pages, Some(2));

        let legacy = b"header\nreserved\n8\nabcdef0123\nunused\nunused\n0\n3\n";
        let legacy = parse_spider_info(legacy).unwrap();
        assert_eq!(legacy.gid, Some(8));
        assert_eq!(legacy.token.as_deref(), Some("abcdef0123"));
        assert_eq!(legacy.declared_pages, Some(3));

        for bytes in [
            Vec::new(),
            vec![b'x'; MAX_SPIDER_INFO_BYTES + 1],
            b"malformed".to_vec(),
            spider_bytes(0, "0123456789", 1),
            spider_bytes(1, "012345678G", 1),
            spider_bytes(1, "0123456789", 0),
        ] {
            assert_eq!(parse_spider_info(&bytes), Err(invalid_metadata()));
        }

        let comic = parse_comic_info(&comic_bytes(9, "abcdef0123", 4, "Local title")).unwrap();
        assert_eq!(comic.gid, Some(9));
        assert_eq!(comic.token.as_deref(), Some("abcdef0123"));
        assert_eq!(comic.title.as_deref(), Some("Local title"));
        assert_eq!(comic.declared_pages, Some(4));
        let no_identity = parse_comic_info(b"<ComicInfo><PageCount>-1</PageCount></ComicInfo>")
            .expect("identity is optional");
        assert_eq!(no_identity.gid, None);
        assert_eq!(no_identity.token, None);
        assert_eq!(no_identity.declared_pages, None);
        assert_eq!(parse_comic_info(b"invalid"), Err(invalid_metadata()));

        for prefix in [
            "https://e-hentai.org/g/",
            "https://exhentai.org/g/",
            "https://e-hentai.org/mpv/",
            "https://exhentai.org/mpv/",
        ] {
            assert_eq!(
                parse_gallery_url(&format!("{prefix}7/0123456789/")),
                Some((7, "0123456789".to_owned()))
            );
        }
        for value in [
            "http://e-hentai.org/g/7/0123456789/",
            "https://e-hentai.org/g/0/0123456789/",
            "https://e-hentai.org/g/x/0123456789/",
            "https://e-hentai.org/g/7/short/",
            "https://e-hentai.org/g/7/012345678G/",
        ] {
            assert_eq!(parse_gallery_url(value), None);
        }
    }

    #[test]
    fn candidate_inspection_covers_ready_conflict_missing_and_archive_states() {
        let mut ready = candidate_request(RecoverySourceKind::Directory);
        ready.spider_info = Some(ByteBuf::from(spider_bytes(7, "0123456789", 2)));
        ready.entries = vec![entry("00000001.jpg"), entry("00000002.png")];
        let ready = inspect_recovery_candidate(ready).unwrap();
        assert_eq!(ready.status, RecoveryStatus::Ready);
        assert_eq!(ready.action, RecoveryPersistenceAction::PromoteFinished);
        assert_eq!(ready.local_page_count, 2);
        assert_eq!(ready.evidence.len(), 2);
        assert!(ready.issues.is_empty());

        let mut conflict = candidate_request(RecoverySourceKind::Directory);
        conflict.spider_info = Some(ByteBuf::from(spider_bytes(7, "0123456789", 2)));
        conflict.comic_info = Some(ByteBuf::from(comic_bytes(8, "abcdef0123", 3, "Title")));
        conflict.entries = vec![entry("00000001.jpg"), entry("00000002.jpg")];
        let conflict = inspect_recovery_candidate(conflict).unwrap();
        assert_eq!(conflict.status, RecoveryStatus::Conflict);
        assert_eq!(conflict.gid, None);
        assert_eq!(conflict.token, None);
        assert_eq!(conflict.declared_pages, None);
        assert!(conflict.issues.contains(&RecoveryIssue::IdentityConflict));
        assert!(conflict.issues.contains(&RecoveryIssue::PageCountConflict));

        let mut invalid = candidate_request(RecoverySourceKind::Directory);
        invalid.spider_info = Some(ByteBuf::from(b"bad".to_vec()));
        invalid.comic_info = Some(ByteBuf::from(b"bad".to_vec()));
        let invalid = inspect_recovery_candidate(invalid).unwrap();
        assert_eq!(invalid.status, RecoveryStatus::MissingIdentity);
        assert!(invalid.issues.contains(&RecoveryIssue::InvalidMetadata));
        assert!(invalid.issues.contains(&RecoveryIssue::MissingGid));
        assert!(invalid.issues.contains(&RecoveryIssue::MissingToken));
        assert!(invalid.issues.contains(&RecoveryIssue::UnknownPageCount));

        let mut unreadable = candidate_request(RecoverySourceKind::Archive);
        unreadable.spider_info = Some(ByteBuf::from(spider_bytes(7, "0123456789", 1)));
        unreadable.entries = vec![entry("page.jpg")];
        let archive = unreadable.archive.as_mut().expect("archive facts");
        archive.complete = false;
        archive.issues = vec![
            RecoveryIssue::EncryptedArchive,
            RecoveryIssue::UnsafeArchivePath,
        ];
        let unreadable = inspect_recovery_candidate(unreadable).unwrap();
        assert_eq!(unreadable.status, RecoveryStatus::Unreadable);
        assert!(
            unreadable
                .issues
                .contains(&RecoveryIssue::UnreadableArchive)
        );

        let mut limited = candidate_request(RecoverySourceKind::Archive);
        limited.archive.as_mut().expect("archive facts").issues =
            vec![RecoveryIssue::LimitExceeded];
        let limited = inspect_recovery_candidate(limited).unwrap();
        assert_eq!(limited.status, RecoveryStatus::LimitExceeded);

        let mut generated = candidate_request(RecoverySourceKind::Archive);
        generated.spider_info = Some(ByteBuf::from(spider_bytes(7, "0123456789", 1)));
        generated.archive.as_mut().expect("archive facts").format = "generatedCbz".to_owned();
        generated.entries = vec![entry("00000001.webp")];
        assert_eq!(
            inspect_recovery_candidate(generated).unwrap().status,
            RecoveryStatus::Ready
        );
    }

    #[test]
    fn candidate_validation_and_envelopes_cover_every_request_shape() {
        let valid = candidate_request(RecoverySourceKind::Directory);
        assert_eq!(validate_candidate_request(&valid), Ok(()));
        let success = inspect_recovery_candidate_envelope(valid.clone());
        assert_eq!(success.version, DOWNLOAD_RECOVERY_API_VERSION);
        assert_eq!(success.module, DOWNLOAD_RECOVERY_MODULE);
        assert!(success.payload.is_some());
        assert_eq!(success.error, None);

        let mut invalid = Vec::new();
        let mut value = valid.clone();
        value.version = 2;
        invalid.push(value);
        let mut value = valid.clone();
        value.source.root_id.clear();
        invalid.push(value);
        let mut value = valid.clone();
        value.source.source_id.clear();
        invalid.push(value);
        let mut value = valid.clone();
        value.archive = Some(archive_input());
        invalid.push(value);
        let mut value = candidate_request(RecoverySourceKind::Archive);
        value.archive = None;
        invalid.push(value);
        let mut value = candidate_request(RecoverySourceKind::Archive);
        value.archive.as_mut().expect("archive").format.clear();
        invalid.push(value);
        let mut value = candidate_request(RecoverySourceKind::Archive);
        value.archive.as_mut().expect("archive").issues = vec![RecoveryIssue::InvalidMetadata];
        invalid.push(value);
        let mut value = valid;
        value.entries = vec![entry("00000001.jpg"); MAX_LOCAL_ENTRIES + 1];
        invalid.push(value);

        for request in invalid {
            assert_eq!(
                validate_candidate_request(&request),
                Err(RustError::Error(
                    "invalid download recovery candidate request".to_owned()
                ))
            );
        }
        let mut wrong_version = candidate_request(RecoverySourceKind::Directory);
        wrong_version.version = 2;
        let failure = inspect_recovery_candidate_envelope(wrong_version);
        assert!(failure.payload.is_none());
        assert_eq!(
            failure.error,
            Some(RustError::Error(
                "invalid download recovery candidate request".to_owned()
            ))
        );
    }

    #[test]
    fn conflict_planner_covers_duplicate_sources_invalid_inputs_and_envelopes() {
        let duplicate_gid = plan_recovery_conflicts(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![ready_candidate("b", 7), ready_candidate("a", 7)],
            existing: Vec::new(),
        })
        .unwrap();
        assert_eq!(
            duplicate_gid
                .decisions
                .iter()
                .map(|decision| decision.source_key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(duplicate_gid.decisions.iter().all(|decision| {
            decision.status == RecoveryStatus::Conflict
                && decision.action == RecoveryPersistenceAction::Stage
                && decision.issue == Some(RecoveryIssue::DuplicateSource)
        }));

        let ignored_incomplete = plan_recovery_conflicts(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![ready_candidate("current", 8)],
            existing: vec![ExistingRecoveryMapping {
                gid: 8,
                source_key: "other".to_owned(),
                complete: false,
            }],
        })
        .unwrap();
        assert_eq!(
            ignored_incomplete.decisions[0].action,
            RecoveryPersistenceAction::PromoteFinished
        );

        let mut staged = ready_candidate("staged", 9);
        staged.status = RecoveryStatus::Incomplete;
        staged.action = RecoveryPersistenceAction::Stage;
        let staged = plan_recovery_conflicts(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![staged],
            existing: Vec::new(),
        })
        .unwrap();
        assert_eq!(staged.decisions[0].status, RecoveryStatus::Incomplete);
        assert_eq!(staged.decisions[0].issue, None);

        let invalid_version = RecoveryConflictRequest {
            version: 2,
            candidates: Vec::new(),
            existing: Vec::new(),
        };
        assert!(plan_recovery_conflicts(invalid_version.clone()).is_err());
        let invalid_gid = RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: Vec::new(),
            existing: vec![ExistingRecoveryMapping {
                gid: 0,
                source_key: "source".to_owned(),
                complete: true,
            }],
        };
        assert!(plan_recovery_conflicts(invalid_gid).is_err());
        let duplicate_source = RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: vec![ready_candidate("same", 1), ready_candidate("same", 2)],
            existing: Vec::new(),
        };
        assert_eq!(
            plan_recovery_conflicts(duplicate_source),
            Err(RustError::Error(
                "duplicate download recovery source key".to_owned()
            ))
        );

        let failure = plan_recovery_conflicts_envelope(invalid_version);
        assert!(failure.payload.is_none());
        assert!(failure.error.is_some());
        let success = plan_recovery_conflicts_envelope(RecoveryConflictRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            candidates: Vec::new(),
            existing: Vec::new(),
        });
        assert!(success.payload.is_some());
        assert_eq!(success.error, None);
    }

    #[test]
    fn fingerprints_reconciliation_and_status_precedence_are_deterministic() {
        let base = RecoveryFingerprintInput {
            size: Some(1),
            modified_at_millis: Some(-2),
            content_digest: Some(ByteBuf::from(vec![0, 1, 2])),
        };
        assert!(fingerprint_matches(&base, &base));
        let mut changed = base.clone();
        changed.size = Some(2);
        assert!(!fingerprint_matches(&base, &changed));
        changed = base.clone();
        changed.modified_at_millis = None;
        changed.content_digest = None;
        assert!(!fingerprint_matches(&base, &changed));

        let directory = RecoverySourceKeyInput {
            root_id: "root".to_owned(),
            source_id: "source".to_owned(),
            source_kind: RecoverySourceKind::Directory,
        };
        let mut archive = directory.clone();
        archive.source_kind = RecoverySourceKind::Archive;
        assert_ne!(source_key(&directory), source_key(&archive));
        assert_eq!(source_key(&directory).len(), 64);

        let evidence = vec![
            normalized_evidence(Some(1), Some("0123456789"), Some(2)),
            normalized_evidence(Some(1), Some("0123456789"), Some(2)),
        ];
        let mut issues = BTreeSet::new();
        assert_eq!(reconcile(&evidence, |item| item.gid, &mut issues), Some(1));
        assert_eq!(
            reconcile_owned(&evidence, |item| item.token.as_ref(), &mut issues).as_deref(),
            Some("0123456789")
        );
        assert_eq!(
            reconcile_page_counts(&evidence, |item| item.declared_pages, &mut issues),
            Some(2)
        );
        assert!(issues.is_empty());
        let conflicts = vec![
            normalized_evidence(Some(1), Some("0123456789"), Some(2)),
            normalized_evidence(Some(2), Some("abcdef0123"), Some(3)),
        ];
        assert_eq!(reconcile(&conflicts, |item| item.gid, &mut issues), None);
        assert_eq!(
            reconcile_owned(&conflicts, |item| item.token.as_ref(), &mut issues),
            None
        );
        assert_eq!(
            reconcile_page_counts(&conflicts, |item| item.declared_pages, &mut issues),
            None
        );
        assert!(issues.contains(&RecoveryIssue::IdentityConflict));
        assert!(issues.contains(&RecoveryIssue::PageCountConflict));
        let empty = Vec::<RecoveryEvidence>::new();
        assert_eq!(reconcile(&empty, |item| item.gid, &mut issues), None);

        let cases = [
            (vec![], RecoveryStatus::Ready),
            (
                vec![RecoveryIssue::InvalidMetadata],
                RecoveryStatus::Incomplete,
            ),
            (
                vec![RecoveryIssue::MissingGid],
                RecoveryStatus::MissingIdentity,
            ),
            (
                vec![RecoveryIssue::IdentityConflict],
                RecoveryStatus::Conflict,
            ),
            (
                vec![RecoveryIssue::UnreadableArchive],
                RecoveryStatus::Unreadable,
            ),
            (
                vec![RecoveryIssue::LimitExceeded],
                RecoveryStatus::LimitExceeded,
            ),
            (
                vec![RecoveryIssue::SourceMissing],
                RecoveryStatus::SourceMissing,
            ),
            (
                vec![RecoveryIssue::SourceMissing, RecoveryIssue::LimitExceeded],
                RecoveryStatus::SourceMissing,
            ),
        ];
        for (issues, expected) in cases {
            assert_eq!(status_for(&issues), expected);
        }
    }

    #[test]
    fn page_and_legacy_helpers_cover_invalid_duplicate_and_temporary_inputs() {
        let mut invalid = entry("00000002.jpg");
        invalid.regular = false;
        let mut encrypted = entry("00000003.jpg");
        encrypted.encrypted = true;
        let numbered = summarize_numbered_pages(&[
            entry("00000001.jpg"),
            entry("00000001.jpg"),
            entry("leftover.TMP"),
            invalid,
            encrypted,
            entry("not-a-page.jpg"),
            entry("00000004.txt"),
        ]);
        assert!(numbered.issues.contains(&RecoveryIssue::DuplicatePage));
        assert!(numbered.issues.contains(&RecoveryIssue::TemporaryFile));
        assert!(numbered.issues.contains(&RecoveryIssue::InvalidImage));
        assert!(!numbered.is_exact(2));

        let mut unreadable = entry("page.png");
        unreadable.readable = false;
        let archive = summarize_archive_images(&[
            entry("chapter/page.jpg"),
            entry("notes.txt"),
            entry("leftover.tmp"),
            unreadable,
        ]);
        assert_eq!(archive.valid_count, 1);
        assert!(archive.issues.contains(&RecoveryIssue::TemporaryFile));
        assert!(archive.issues.contains(&RecoveryIssue::InvalidImage));

        assert_eq!(strict_page_number("00000001.jpg"), Some(1));
        for name in [
            "folder/00000001.jpg",
            "folder\\00000001.jpg",
            "00000001",
            "0000001.jpg",
            "0000000x.jpg",
            "00000001.JPG",
            "00000000.jpg",
        ] {
            assert_eq!(strict_page_number(name), None, "{name}");
        }
        assert_eq!(playable_image_extension("page.avif"), Some("avif"));
        assert_eq!(playable_image_extension("page.txt"), None);
        assert_eq!(playable_image_extension("page"), None);
        assert!(temporary_file("leftover.TmP"));
        assert!(!temporary_file("page.jpg"));

        assert!(crate::spider_info::decode_identity(&[0xff]).is_err());
        assert!(crate::spider_info::decode_identity(b"too\nshort\n").is_err());
        assert!(
            crate::spider_info::decode_identity(b"h\nr\nnot-a-gid\ntoken\na\nb\n0\n1\n").is_err()
        );
        assert!(!valid_token("short"));
        assert!(!valid_token("012345678G"));
        assert!(valid_token("abcdef0123"));
    }

    fn spider_bytes(gid: i64, token: &str, pages: i32) -> Vec<u8> {
        #[derive(serde::Serialize)]
        struct SpiderFixture<'a> {
            gid: i64,
            token: &'a str,
            pages: i32,
        }
        serde_cbor::to_vec(&SpiderFixture { gid, token, pages }).unwrap()
    }

    fn comic_bytes(gid: i64, token: &str, pages: i32, title: &str) -> Vec<u8> {
        format!(
            "<ComicInfo><Series>{title}</Series><Web>https://e-hentai.org/g/{gid}/{token}/</Web><PageCount>{pages}</PageCount></ComicInfo>"
        )
        .into_bytes()
    }

    fn archive_input() -> RecoveryArchiveInput {
        RecoveryArchiveInput {
            format: "zip".to_owned(),
            complete: true,
            issues: Vec::new(),
        }
    }

    fn candidate_request(source_kind: RecoverySourceKind) -> RecoveryCandidateRequest {
        RecoveryCandidateRequest {
            version: DOWNLOAD_RECOVERY_API_VERSION,
            source: RecoverySourceKeyInput {
                root_id: "root".to_owned(),
                source_id: "source".to_owned(),
                source_kind,
            },
            fingerprint: RecoveryFingerprintInput {
                size: Some(1),
                modified_at_millis: Some(2),
                content_digest: None,
            },
            display_name: "candidate".to_owned(),
            spider_info: None,
            comic_info: None,
            entries: Vec::new(),
            archive: (source_kind == RecoverySourceKind::Archive).then(archive_input),
        }
    }

    fn normalized_evidence(
        gid: Option<i64>,
        token: Option<&str>,
        declared_pages: Option<u32>,
    ) -> RecoveryEvidence {
        RecoveryEvidence {
            kind: RecoveryEvidenceKind::SpiderInfo,
            gid,
            token: token.map(ToOwned::to_owned),
            title: None,
            declared_pages,
            observed_pages: None,
        }
    }

    fn entry(name: &str) -> RecoveryEntryInput {
        RecoveryEntryInput {
            name: name.to_owned(),
            size: 1,
            regular: true,
            encrypted: false,
            readable: true,
        }
    }

    fn ready_candidate(source_key: &str, gid: i64) -> RecoveryCandidatePlan {
        RecoveryCandidatePlan {
            source_key: source_key.to_owned(),
            fingerprint: "fingerprint".to_owned(),
            source_kind: RecoverySourceKind::Archive,
            display_name: "candidate.cbz".to_owned(),
            gid: Some(gid),
            token: Some("0123456789".to_owned()),
            title: None,
            declared_pages: Some(1),
            local_page_count: 1,
            evidence: Vec::new(),
            issues: Vec::new(),
            status: RecoveryStatus::Ready,
            action: RecoveryPersistenceAction::PromoteFinished,
        }
    }

    fn decode_hex(input: &str) -> Vec<u8> {
        input
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    const KOTLIN_BYTE_STRING_REQUEST_CBOR_HEX: &str = concat!(
        "a66776657273696f6e0166736f75726365a366726f6f744964617268736f7572636549646173",
        "6a736f757263654b696e64696469726563746f72796b66696e6765727072696e74a16d636f6e",
        "74656e74446967657374430080ff6b646973706c61794e616d6561646a737069646572496e66",
        "6f430080ff69636f6d6963496e666f430080ff"
    );
}
