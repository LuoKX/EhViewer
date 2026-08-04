//! Pure runtime progress planning for one active Android download.
//!
//! Android retains workers, transfer maps, persistence, and listeners. This module
//! only projects immutable counters and transfer observations into a snapshot.

use serde::{Deserialize, Serialize};

use super::DownloadState;

/// Protocol version for the download-progress contract.
pub const DOWNLOAD_PROGRESS_API_VERSION: u8 = 1;
/// Stable module identifier for download-progress responses.
pub const DOWNLOAD_PROGRESS_MODULE: &str = "downloadProgress";

/// Immutable runtime download counters and estimates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressSnapshot {
    /// Current download lifecycle state.
    pub state: DownloadState,
    /// Aggregate speed in bytes per second, or `-1` before measurement.
    pub speed_bytes_per_second: i64,
    /// Estimated remaining duration in milliseconds, or `-1` when unavailable.
    pub remaining_milliseconds: i64,
    /// Number of completed pages.
    pub finished: i32,
    /// Number of downloaded pages.
    pub downloaded: i32,
    /// Total page count, or `-1` before page metadata arrives.
    pub total: i32,
    /// Legacy page counter maintained with signed wrapping arithmetic.
    pub legacy: i32,
}

/// One active transfer observation used for ETA projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDownloadTransfer {
    /// Expected transfer byte length.
    pub content_length: i64,
    /// Bytes received so far.
    pub received_size: i64,
}

/// One deterministic progress command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DownloadProgressCommand {
    /// Resets runtime counters when a worker starts.
    Reset,
    /// Updates the page total without changing other counters.
    SetPages {
        /// New total page count.
        total: i32,
    },
    /// Updates page completion counters without changing other counters.
    SetPageResult {
        /// Completed page count.
        finished: i32,
        /// Downloaded page count.
        downloaded: i32,
        /// Total page count.
        total: i32,
    },
    /// Updates speed and computes an ETA from active transfers.
    Estimate {
        /// Aggregate speed in bytes per second.
        speed_bytes_per_second: i64,
        /// Active transfer observations.
        transfers: Vec<ActiveDownloadTransfer>,
    },
    /// Applies terminal counters and chooses Finished or Failed state.
    Finish {
        /// Completed page count.
        finished: i32,
        /// Downloaded page count.
        downloaded: i32,
        /// Total page count.
        total: i32,
        /// Whether the terminal state is failed.
        failed: bool,
    },
}

/// Versioned progress request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressRequest {
    /// Protocol version; callers must use [`DOWNLOAD_PROGRESS_API_VERSION`].
    pub version: u8,
    /// Snapshot before applying the command.
    pub snapshot: DownloadProgressSnapshot,
    /// Progress command to apply.
    pub command: DownloadProgressCommand,
}

/// Deterministic progress result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressPlan {
    /// Snapshot after applying the command.
    pub snapshot: DownloadProgressSnapshot,
}

/// Plans one progress command with JVM-compatible signed overflow semantics.
#[must_use]
pub fn plan_download_progress(
    mut snapshot: DownloadProgressSnapshot,
    command: DownloadProgressCommand,
) -> DownloadProgressPlan {
    match command {
        DownloadProgressCommand::Reset => {
            snapshot.state = DownloadState::Downloading;
            snapshot.speed_bytes_per_second = -1;
            snapshot.remaining_milliseconds = -1;
            snapshot.total = -1;
            snapshot.finished = 0;
            snapshot.downloaded = 0;
            snapshot.legacy = -1;
        }
        DownloadProgressCommand::SetPages { total } => snapshot.total = total,
        DownloadProgressCommand::SetPageResult {
            finished,
            downloaded,
            total,
        } => {
            snapshot.finished = finished;
            snapshot.downloaded = downloaded;
            snapshot.total = total;
        }
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second,
            transfers,
        } => {
            snapshot.speed_bytes_per_second = speed_bytes_per_second;
            snapshot.remaining_milliseconds = if snapshot.total <= 0 {
                -1
            } else if speed_bytes_per_second == 0 {
                300_i64 * 24 * 60 * 60 * 1000
            } else if transfers.is_empty() {
                snapshot.remaining_milliseconds
            } else {
                // Kotlin's collection size is an Int; retain JVM low-bit conversion for
                // pathological host-only vectors while keeping ordinary sizes exact.
                #[allow(clippy::manual_unwrap_or)]
                let count = match i64::try_from(transfers.len()) {
                    Ok(value) => value,
                    Err(_) => i64::MAX,
                };
                #[allow(clippy::manual_unwrap_or)]
                let count_i32 = i32::from_ne_bytes(
                    match u32::try_from(transfers.len()) {
                        Ok(value) => value,
                        Err(_) => u32::MAX,
                    }
                    .to_ne_bytes(),
                );
                let mut remaining = 0_i64;
                let mut content_sum = 0_i64;
                for transfer in transfers {
                    remaining = remaining
                        .wrapping_add(transfer.content_length.wrapping_sub(transfer.received_size));
                    content_sum = content_sum.wrapping_add(transfer.content_length);
                }
                let pages = i64::from(
                    snapshot
                        .total
                        .wrapping_sub(snapshot.downloaded)
                        .wrapping_sub(count_i32),
                );
                remaining =
                    remaining.wrapping_add(content_sum.wrapping_mul(pages).wrapping_div(count));
                wrapping_div(remaining, speed_bytes_per_second).wrapping_mul(1000)
            };
        }
        DownloadProgressCommand::Finish {
            finished,
            downloaded,
            total,
            failed,
        } => {
            snapshot.finished = finished;
            snapshot.downloaded = downloaded;
            snapshot.total = total;
            snapshot.legacy = total.wrapping_sub(finished);
            snapshot.state = if failed {
                DownloadState::Failed
            } else {
                DownloadState::Finished
            };
        }
    }
    DownloadProgressPlan { snapshot }
}

fn wrapping_div(left: i64, right: i64) -> i64 {
    if left == i64::MIN && right == -1 {
        i64::MIN
    } else {
        left / right
    }
}

#[cfg(test)]
mod tests;
