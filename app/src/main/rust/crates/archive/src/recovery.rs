//! Recovery-only archive entry policy shared by every supported codec.
//!
//! Codec adapters enumerate and fully read entries; this module validates their
//! bounded facts without receiving Android paths, handles, or passwords.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Root metadata filename written by EhViewer download directories.
pub const SPIDER_INFO_ENTRY_NAME: &str = ".ehviewer";
/// Root metadata filename written by EhViewer and common comic tools.
pub const COMIC_INFO_ENTRY_NAME: &str = "ComicInfo.xml";

const IMAGE_EXTENSIONS: [&str; 11] = [
    "jpeg", "jpg", "png", "gif", "webp", "bmp", "ico", "wbmp", "heic", "heif", "avif",
];

/// Content-detected archive kind accepted by recovery inspection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryArchiveFormat {
    /// ZIP or CBZ container.
    Zip,
    /// RAR4 container.
    Rar,
    /// RAR5 container.
    Rar5,
    /// 7z container.
    SevenZip,
    /// Uncompressed tar container.
    Tar,
    /// Gzip-compressed tar container.
    TarGzip,
    /// XZ-compressed tar container.
    TarXz,
}

/// Centralized resource limits for one archive inspection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveLimits {
    /// Maximum enumerated headers.
    pub maximum_entries: u32,
    /// Maximum UTF-8 bytes in one entry name.
    pub maximum_name_bytes: u32,
    /// Maximum path component depth.
    pub maximum_path_depth: u16,
    /// Maximum uncompressed bytes in one entry.
    pub maximum_entry_bytes: u64,
    /// Maximum uncompressed bytes read across the archive.
    pub maximum_total_bytes: u64,
    /// Maximum bytes retained for one whitelisted metadata entry.
    pub maximum_metadata_bytes: u32,
    /// Maximum permitted uncompressed-to-compressed ratio.
    pub maximum_compression_ratio: u32,
}

impl Default for RecoveryArchiveLimits {
    fn default() -> Self {
        Self {
            maximum_entries: 100_000,
            maximum_name_bytes: 4_096,
            maximum_path_depth: 32,
            maximum_entry_bytes: 512 * 1024 * 1024,
            maximum_total_bytes: 4 * 1024 * 1024 * 1024,
            maximum_metadata_bytes: 512 * 1024,
            maximum_compression_ratio: 1_000,
        }
    }
}

/// One fully enumerated codec entry supplied to the shared recovery policy.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveEntry {
    /// Complete archive-relative entry name.
    pub name: String,
    /// Declared compressed bytes when the codec exposes them.
    pub compressed_size: Option<u64>,
    /// Declared uncompressed bytes.
    pub uncompressed_size: u64,
    /// Whether the entry is a regular file.
    pub regular: bool,
    /// Whether the entry is encrypted.
    pub encrypted: bool,
    /// Whether decoding to EOF and integrity/CRC validation succeeded.
    pub readable: bool,
    /// Contents retained only for an exact root metadata whitelist match.
    #[serde(default)]
    pub metadata: Option<Vec<u8>>,
}

/// Stable issue emitted by archive recovery policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryArchiveIssue {
    /// At least one encrypted entry was observed.
    Encrypted,
    /// At least one regular entry could not be decoded and integrity checked.
    Unreadable,
    /// An absolute, parent-traversal, drive-prefixed, or empty path was observed.
    UnsafePath,
    /// An entry-count, name, depth, size, ratio, or total-read budget was exceeded.
    LimitExceeded,
    /// The same exact root metadata name occurred more than once.
    DuplicateMetadata,
    /// More than one archive entry has the same complete name.
    DuplicateName,
}

/// One safe playable image retained in natural entry-name order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveImage {
    /// Complete safe archive-relative name.
    pub name: String,
    /// Uncompressed byte size.
    pub size: u64,
}

/// Exact root metadata retained within the small-entry budget.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveMetadata {
    /// Exact whitelisted root filename.
    pub name: String,
    /// Fully decoded bytes.
    pub bytes: Vec<u8>,
}

/// Complete recovery inspection summary independent of a codec implementation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryArchiveInspection {
    /// Content-detected archive format.
    pub format: RecoveryArchiveFormat,
    /// Total enumerated headers.
    pub entry_count: u32,
    /// Checked sum of declared uncompressed regular-file bytes.
    pub total_uncompressed_bytes: u64,
    /// Natural-sorted playable image entries.
    pub images: Vec<RecoveryArchiveImage>,
    /// At most one of each exact root metadata filename.
    pub metadata: Vec<RecoveryArchiveMetadata>,
    /// Stable, deduplicated issues.
    pub issues: Vec<RecoveryArchiveIssue>,
}

/// Request-level archive inspection failure.
#[derive(Debug, Error)]
pub enum RecoveryArchiveError {
    /// The caller supplied limits that cannot safely inspect any archive.
    #[error("archive recovery limits are invalid")]
    InvalidLimits,
    /// Counting bytes overflowed the stable summary range.
    #[error("archive recovery byte count overflow")]
    ByteCountOverflow,
}

/// Applies shared path, metadata, image, readability, and resource-limit policy.
///
/// The caller must decode every entry to EOF before marking it readable. Encrypted
/// input is classified, never retried with a password by this recovery boundary.
///
/// # Errors
///
/// Returns [`RecoveryArchiveError`] only for invalid caller limits or integer
/// overflow. Untrusted archive problems are returned as structured issues.
pub fn inspect_recovery_entries(
    format: RecoveryArchiveFormat,
    entries: Vec<RecoveryArchiveEntry>,
    limits: RecoveryArchiveLimits,
) -> Result<RecoveryArchiveInspection, RecoveryArchiveError> {
    if limits.maximum_entries == 0
        || limits.maximum_name_bytes == 0
        || limits.maximum_path_depth == 0
        || limits.maximum_entry_bytes == 0
        || limits.maximum_total_bytes == 0
        || limits.maximum_metadata_bytes == 0
        || limits.maximum_compression_ratio == 0
    {
        return Err(RecoveryArchiveError::InvalidLimits);
    }
    let mut issues = std::collections::BTreeSet::new();
    if entries.len() > usize::try_from(limits.maximum_entries).unwrap_or(usize::MAX) {
        issues.insert(RecoveryArchiveIssue::LimitExceeded);
    }
    let mut names = std::collections::BTreeSet::new();
    let mut metadata_names = std::collections::BTreeSet::new();
    let mut images = Vec::new();
    let mut metadata = Vec::new();
    let mut total_uncompressed_bytes = 0_u64;
    for entry in entries
        .iter()
        .take(usize::try_from(limits.maximum_entries).unwrap_or(usize::MAX))
    {
        if entry.name.len() > usize::try_from(limits.maximum_name_bytes).unwrap_or(usize::MAX)
            || path_depth(&entry.name) > usize::from(limits.maximum_path_depth)
            || entry.uncompressed_size > limits.maximum_entry_bytes
            || compression_ratio_exceeded(entry, limits.maximum_compression_ratio)
        {
            issues.insert(RecoveryArchiveIssue::LimitExceeded);
            continue;
        }
        if !safe_relative_path(&entry.name) {
            issues.insert(RecoveryArchiveIssue::UnsafePath);
            continue;
        }
        if !names.insert(entry.name.clone()) {
            issues.insert(RecoveryArchiveIssue::DuplicateName);
        }
        if !entry.regular {
            continue;
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry.uncompressed_size)
            .ok_or(RecoveryArchiveError::ByteCountOverflow)?;
        if total_uncompressed_bytes > limits.maximum_total_bytes {
            issues.insert(RecoveryArchiveIssue::LimitExceeded);
        }
        if entry.encrypted {
            issues.insert(RecoveryArchiveIssue::Encrypted);
        }
        if !entry.readable {
            issues.insert(RecoveryArchiveIssue::Unreadable);
        }
        if is_root_metadata(&entry.name) {
            if !metadata_names.insert(entry.name.clone()) {
                issues.insert(RecoveryArchiveIssue::DuplicateMetadata);
            }
            if entry.uncompressed_size > u64::from(limits.maximum_metadata_bytes) {
                issues.insert(RecoveryArchiveIssue::LimitExceeded);
            } else if let Some(bytes) = entry.metadata.as_ref() {
                if bytes.len() == usize::try_from(entry.uncompressed_size).unwrap_or(usize::MAX)
                    && entry.readable
                    && !entry.encrypted
                {
                    metadata.push(RecoveryArchiveMetadata {
                        name: entry.name.clone(),
                        bytes: bytes.clone(),
                    });
                } else {
                    issues.insert(RecoveryArchiveIssue::Unreadable);
                }
            }
        }
        if entry.readable
            && !entry.encrypted
            && entry.uncompressed_size > 0
            && playable_extension(&entry.name).is_some()
        {
            images.push(RecoveryArchiveImage {
                name: entry.name.clone(),
                size: entry.uncompressed_size,
            });
        }
    }
    images.sort_by(|left, right| natural_compare(&left.name, &right.name));
    metadata.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(RecoveryArchiveInspection {
        format,
        entry_count: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        total_uncompressed_bytes,
        images,
        metadata,
        issues: issues.into_iter().collect(),
    })
}

/// Returns whether an entry path is relative, normalized, and non-escaping.
#[must_use]
pub fn safe_relative_path(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.as_bytes().get(1) == Some(&b':')
    {
        return false;
    }
    !name
        .split(['/', '\\'])
        .any(|component| component.is_empty() || component == "." || component == "..")
}

fn is_root_metadata(name: &str) -> bool {
    name == SPIDER_INFO_ENTRY_NAME || name == COMIC_INFO_ENTRY_NAME
}

fn path_depth(name: &str) -> usize {
    name.split(['/', '\\']).count()
}

fn compression_ratio_exceeded(entry: &RecoveryArchiveEntry, maximum: u32) -> bool {
    match entry.compressed_size {
        Some(0) => entry.uncompressed_size > 0,
        Some(compressed) => entry.uncompressed_size / compressed > u64::from(maximum),
        None => false,
    }
}

fn playable_extension(name: &str) -> Option<&str> {
    let extension = name.rsplit_once('.')?.1;
    IMAGE_EXTENSIONS.contains(&extension).then_some(extension)
}

fn natural_compare(left: &str, right: &str) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let left_number = trim_digit_run(&left[left_index..left_end]);
            let right_number = trim_digit_run(&right[right_index..right_end]);
            let ordering = left_number
                .len()
                .cmp(&right_number.len())
                .then_with(|| left_number.cmp(right_number))
                .then_with(|| (left_end - left_index).cmp(&(right_end - right_index)));
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
        } else {
            let ordering = left[left_index].cmp(&right[right_index]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index += 1;
            right_index += 1;
        }
    }
    left.len().cmp(&right.len())
}

fn digit_run_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    index
}

fn trim_digit_run(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|byte| *byte != b'0')
        .map_or(&bytes[bytes.len() - 1..], |index| &bytes[index..])
}
