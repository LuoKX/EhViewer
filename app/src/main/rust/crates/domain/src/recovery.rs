//! Pure download-directory recovery contracts and deterministic planning.
//!
//! Android owns traversal, SAF handles, Room transactions, WorkManager, and all
//! network access. This module receives bounded bytes and entry facts, reconciles
//! local evidence, proves page completeness, and returns one persistence intent.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Cursor;

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::contracts::{ApiEnvelope, RustError};

/// Version of the download-recovery CBOR contract.
pub const DOWNLOAD_RECOVERY_API_VERSION: u8 = 1;
/// Stable module identifier for download-recovery envelopes.
pub const DOWNLOAD_RECOVERY_MODULE: &str = "downloadRecovery";
/// Maximum accepted `.ehviewer` metadata length.
pub const MAX_SPIDER_INFO_BYTES: usize = 512 * 1024;
/// Maximum accepted root `ComicInfo.xml` length.
pub const MAX_COMIC_INFO_BYTES: usize = 512 * 1024;
/// Maximum number of local entry facts accepted for one candidate.
pub const MAX_LOCAL_ENTRIES: usize = 100_000;

const IMAGE_EXTENSIONS: [&str; 11] = [
    "jpeg", "jpg", "png", "gif", "webp", "bmp", "ico", "wbmp", "heic", "heif", "avif",
];

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
    pub content_digest: Option<Vec<u8>>,
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
    /// Whether any archive entry is encrypted.
    pub encrypted: bool,
    /// Whether any unsafe absolute or parent-traversal path was observed.
    pub unsafe_path: bool,
    /// Whether a resource budget was exhausted.
    pub limit_exceeded: bool,
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
    pub spider_info: Option<Vec<u8>>,
    /// Optional bounded root `ComicInfo.xml` bytes.
    #[serde(default)]
    pub comic_info: Option<Vec<u8>>,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpiderInfoWire {
    gid: i64,
    token: String,
    pages: i32,
}

/// Parses bounded current-CBOR or legacy-v2 `.ehviewer` metadata.
///
/// # Errors
///
/// Returns a fixed, token-free error for oversized, malformed, or invalid metadata.
pub fn parse_spider_info(bytes: &[u8]) -> Result<RecoveryEvidence, RustError> {
    if bytes.is_empty() || bytes.len() > MAX_SPIDER_INFO_BYTES {
        return Err(invalid_metadata());
    }
    let parsed = serde_cbor::from_slice::<SpiderInfoWire>(bytes)
        .ok()
        .or_else(|| parse_legacy_spider_info(bytes));
    let parsed = parsed.ok_or_else(invalid_metadata)?;
    if parsed.gid <= 0 || !valid_token(&parsed.token) || parsed.pages <= 0 {
        return Err(invalid_metadata());
    }
    Ok(RecoveryEvidence {
        kind: RecoveryEvidenceKind::SpiderInfo,
        gid: Some(parsed.gid),
        token: Some(parsed.token),
        title: None,
        declared_pages: u32::try_from(parsed.pages).ok(),
        observed_pages: None,
    })
}

/// Parses bounded root `ComicInfo.xml` metadata and an optional strict E/EX URL.
///
/// # Errors
///
/// Returns a fixed, source-free error for malformed or oversized XML.
pub fn parse_comic_info(bytes: &[u8]) -> Result<RecoveryEvidence, RustError> {
    if bytes.is_empty() || bytes.len() > MAX_COMIC_INFO_BYTES {
        return Err(invalid_metadata());
    }
    let mut reader = Reader::from_reader(Cursor::new(bytes));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut current = None::<Vec<u8>>;
    let mut series = None;
    let mut web = None;
    let mut pages = None;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(start)) => current = Some(start.name().as_ref().to_vec()),
            Ok(Event::Text(text)) => {
                let value = text.decode().map_err(|_| invalid_metadata())?.into_owned();
                match current.as_deref() {
                    Some(b"Series") => series = nonblank(value),
                    Some(b"Web") => web = nonblank(value),
                    Some(b"PageCount") => pages = value.parse::<u32>().ok().filter(|v| *v > 0),
                    _ => {}
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return Err(invalid_metadata()),
        }
        buffer.clear();
    }
    let (gid, token) = web.as_deref().and_then(parse_gallery_url).unzip();
    Ok(RecoveryEvidence {
        kind: RecoveryEvidenceKind::ComicInfo,
        gid,
        token,
        title: series,
        declared_pages: pages,
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
    if request.version != DOWNLOAD_RECOVERY_API_VERSION
        || request.source.root_id.is_empty()
        || request.source.source_id.is_empty()
        || request.entries.len() > MAX_LOCAL_ENTRIES
    {
        return Err(RustError::Error(
            "invalid download recovery candidate request".to_owned(),
        ));
    }
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
        if archive.encrypted {
            issues.insert(RecoveryIssue::EncryptedArchive);
        }
        if !archive.complete {
            issues.insert(RecoveryIssue::UnreadableArchive);
        }
        if archive.unsafe_path {
            issues.insert(RecoveryIssue::UnsafeArchivePath);
        }
        if archive.limit_exceeded {
            issues.insert(RecoveryIssue::LimitExceeded);
        }
    } else if request.source.source_kind == RecoverySourceKind::Archive {
        issues.insert(RecoveryIssue::UnreadableArchive);
    }

    let gid = reconcile(&evidence, |item| item.gid, &mut issues);
    let token = reconcile_owned(&evidence, |item| item.token.as_ref(), &mut issues);
    let declared_pages = reconcile_page_counts(
        &evidence,
        |item| item.declared_pages,
        &mut issues,
    );
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

/// Builds a versioned success or sanitized error envelope for one candidate.
#[must_use]
pub fn inspect_recovery_candidate_envelope(
    request: RecoveryCandidateRequest,
) -> ApiEnvelope<RecoveryCandidatePlan> {
    match inspect_recovery_candidate(request) {
        Ok(plan) => ApiEnvelope::ok(DOWNLOAD_RECOVERY_API_VERSION, DOWNLOAD_RECOVERY_MODULE, plan),
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
    request.candidates.sort_by(|a, b| a.source_key.cmp(&b.source_key));
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
        if candidate.status == RecoveryStatus::Ready {
            if let Some(gid) = candidate.gid {
                by_gid.entry(gid).or_default().push(candidate.source_key.clone());
            }
        }
    }
    let decisions = request
        .candidates
        .into_iter()
        .map(|candidate| {
            let mut status = candidate.status;
            let mut action = candidate.action;
            let mut issue = None;
            if status == RecoveryStatus::Ready {
                if let Some(gid) = candidate.gid {
                    if let Some(mapped) = existing.get(&gid) {
                        if mapped != &candidate.source_key {
                            status = RecoveryStatus::Conflict;
                            action = RecoveryPersistenceAction::Stage;
                            issue = Some(RecoveryIssue::ExistingMappingConflict);
                        }
                    } else if by_gid.get(&gid).is_some_and(|sources| sources.len() > 1) {
                        status = RecoveryStatus::Conflict;
                        action = RecoveryPersistenceAction::Stage;
                        issue = Some(RecoveryIssue::DuplicateSource);
                    }
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
        Ok(plan) => ApiEnvelope::ok(DOWNLOAD_RECOVERY_API_VERSION, DOWNLOAD_RECOVERY_MODULE, plan),
        Err(error) => ApiEnvelope::err(
            DOWNLOAD_RECOVERY_API_VERSION,
            DOWNLOAD_RECOVERY_MODULE,
            error,
        ),
    }
}

/// Returns whether two typed fingerprint inputs describe the same source revision.
#[must_use]
pub fn fingerprint_matches(left: &RecoveryFingerprintInput, right: &RecoveryFingerprintInput) -> bool {
    fingerprint(left) == fingerprint(right)
}

#[derive(Default)]
struct PageSummary {
    valid_count: u32,
    ordinals: BTreeMap<u32, u32>,
    issues: BTreeSet<RecoveryIssue>,
}

impl PageSummary {
    fn is_exact(&self, expected: u32) -> bool {
        self.valid_count == expected
            && self.ordinals.len() == usize::try_from(expected).unwrap_or(usize::MAX)
            && (1..=expected).all(|page| self.ordinals.get(&page) == Some(&1))
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
    let mut summary = PageSummary::default();
    for entry in entries {
        if entry.name.ends_with(".tmp") {
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
        if entry.name.ends_with(".tmp") {
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
    if name.contains('/') || name.contains('\\') {
        return None;
    }
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.len() != 8
        || !stem.bytes().all(|byte| byte.is_ascii_digit())
        || !IMAGE_EXTENSIONS.contains(&extension)
    {
        return None;
    }
    stem.parse::<u32>().ok().filter(|page| *page > 0)
}

fn playable_image_extension(name: &str) -> Option<&str> {
    let extension = name.rsplit_once('.')?.1;
    IMAGE_EXTENSIONS.contains(&extension).then_some(extension)
}

fn parse_legacy_spider_info(bytes: &[u8]) -> Option<SpiderInfoWire> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines();
    lines.next()?;
    lines.next()?;
    let gid = lines.next()?.parse().ok()?;
    let token = lines.next()?.to_owned();
    lines.next()?;
    lines.next()?;
    lines.next()?.parse::<i32>().ok()?;
    let pages = lines.next()?.parse().ok()?;
    Some(SpiderInfoWire { gid, token, pages })
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
    let gid = segments.next()?.parse::<i64>().ok().filter(|gid| *gid > 0)?;
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

fn nonblank(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
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
    let size = input.size.map(|value| value.to_be_bytes());
    let modified = input.modified_at_millis.map(|value| value.to_be_bytes());
    digest_parts(&[
        b"fingerprint-v1",
        size.as_ref().map_or(&[][..], <[u8; 8]>::as_slice),
        modified.as_ref().map_or(&[][..], <[u8; 8]>::as_slice),
        input.content_digest.as_deref().unwrap_or_default(),
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
