//! Pure download-queue state transitions.
//!
//! Android supplies a complete queue snapshot and one command. This module
//! validates that snapshot, applies the command deterministically, and returns
//! state changes plus the task identifiers whose Android workers should start or
//! cancel. Persisted entries must first pass through [`recover_download_queue`]
//! after process death. This module never opens a database, starts a service,
//! accesses a destination path, or performs network/file IO.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use serde::{Deserialize, Serialize};

pub mod progress;

/// Protocol version of the Rust download-queue request and response.
pub const DOWNLOAD_QUEUE_API_VERSION: u8 = 1;

/// Stable module identifier used by the download-queue response envelope.
pub const DOWNLOAD_QUEUE_MODULE: &str = "downloadQueue";

/// Persisted lifecycle state of one gallery download.
///
/// The variants preserve the meanings of the existing Android `DOWNLOADS.STATE`
/// values without exposing those integer encodings on the new wire boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadState {
    /// A legacy or corrupted state that bulk commands must ignore.
    Invalid,
    /// The task exists but is neither queued nor active.
    None,
    /// The task is waiting in the FIFO queue.
    Waiting,
    /// The single task currently owned by the Android download worker.
    Downloading,
    /// The task completed successfully.
    Finished,
    /// The task reached a terminal failure and may be retried.
    Failed,
}

impl DownloadState {
    /// Normalizes volatile queue states after an Android process restart.
    ///
    /// Waiting and active workers do not survive process death, so both become
    /// [`Self::None`]. Terminal and invalid persisted states remain unchanged.
    #[must_use]
    pub const fn after_process_restart(self) -> Self {
        match self {
            Self::Waiting | Self::Downloading => Self::None,
            state => state,
        }
    }

    const fn can_start_range(self) -> bool {
        matches!(self, Self::None | Self::Finished | Self::Failed)
    }

    const fn can_start_all(self) -> bool {
        matches!(self, Self::None | Self::Failed)
    }
}

/// One download and its current lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEntry {
    /// Positive remote gallery identifier used as the stable queue key.
    #[serde(rename = "gid")]
    pub gallery_id: i64,
    /// Current persisted/ephemeral lifecycle state.
    pub state: DownloadState,
}

/// Complete platform-independent view of the download scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueueSnapshot {
    /// Downloads in Android's current stable display/start-all order.
    pub entries: Vec<DownloadEntry>,
    /// Positive gallery identifiers in FIFO waiting order.
    pub waiting: Vec<i64>,
    /// The active gallery identifier, or `None` while the worker is idle.
    pub current: Option<i64>,
}

/// One command applied atomically to a [`DownloadQueueSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DownloadCommand {
    /// Starts or retries one existing task, including legacy invalid/finished states.
    StartOne {
        /// Gallery identifier selected by the caller.
        #[serde(rename = "gid")]
        gallery_id: i64,
    },
    /// Starts eligible selected tasks in the caller's order.
    StartRange {
        /// Unique gallery identifiers selected by the caller.
        #[serde(rename = "gids")]
        gallery_ids: Vec<i64>,
    },
    /// Starts every idle or failed task in snapshot entry order.
    StartAll,
    /// Starts the next waiting task only when the worker is idle.
    ActivateNext,
    /// Stops one active or waiting task and advances the queue when necessary.
    StopOne {
        /// Gallery identifier selected by the caller.
        #[serde(rename = "gid")]
        gallery_id: i64,
    },
    /// Stops selected active/waiting tasks, then advances the FIFO once.
    StopRange {
        /// Unique gallery identifiers selected by the caller.
        #[serde(rename = "gids")]
        gallery_ids: Vec<i64>,
    },
    /// Stops the active task and clears the complete waiting queue.
    StopAll,
    /// Records the active worker's terminal result and advances the queue.
    Finish {
        /// Gallery identifier reported by the active worker.
        #[serde(rename = "gid")]
        gallery_id: i64,
        /// Whether the worker reached a failed rather than successful terminal state.
        failed: bool,
    },
}

/// Versioned request sent from the Android scheduler to the Rust reducer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueueRequest {
    /// Protocol version; callers must use [`DOWNLOAD_QUEUE_API_VERSION`].
    pub version: u8,
    /// Complete live scheduler state after process-recovery normalization.
    pub snapshot: DownloadQueueSnapshot,
    /// Atomic command to apply to the snapshot.
    pub command: DownloadCommand,
}

/// Versioned persisted-entry request used after Android process restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueueRecoveryRequest {
    /// Protocol version; callers must use [`DOWNLOAD_QUEUE_API_VERSION`].
    pub version: u8,
    /// Persisted entries in Android's stable display/start-all order.
    pub entries: Vec<DownloadEntry>,
}

/// Final state change for one download produced by a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadStateChange {
    /// Gallery identifier whose state changed.
    #[serde(rename = "gid")]
    pub gallery_id: i64,
    /// State in the validated input snapshot.
    pub previous: DownloadState,
    /// State after the command and automatic FIFO advancement.
    pub current: DownloadState,
}

/// Deterministic result of one download-queue command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQueuePlan {
    /// Complete validated scheduler state after applying the command.
    pub snapshot: DownloadQueueSnapshot,
    /// Final state changes in stable snapshot entry order.
    pub changes: Vec<DownloadStateChange>,
    /// Task whose Android worker must be started, if this command advanced the queue.
    #[serde(rename = "startGid")]
    pub start_gallery_id: Option<i64>,
    /// Active task whose Android worker must be cancelled, if explicitly stopped.
    #[serde(rename = "cancelGid")]
    pub cancel_gallery_id: Option<i64>,
}

/// Structural reason that a queue snapshot or command cannot be reduced safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadQueueError {
    /// A snapshot entry or command used a non-positive gallery identifier.
    NonPositiveGalleryId(i64),
    /// Two snapshot entries used the same gallery identifier.
    DuplicateEntry(i64),
    /// The active identifier did not exist in the entry list.
    MissingCurrent(i64),
    /// The active entry was not in the downloading state.
    CurrentState {
        /// Active gallery identifier.
        gallery_id: i64,
        /// Unexpected state found in the snapshot.
        actual: DownloadState,
    },
    /// An entry was downloading but was not the unique active task.
    UnexpectedDownloading(i64),
    /// A gallery identifier appeared more than once in the waiting FIFO.
    DuplicateWaiting(i64),
    /// A waiting identifier did not exist in the entry list.
    MissingWaiting(i64),
    /// A FIFO entry was not in the waiting state.
    WaitingState {
        /// Gallery identifier present in the FIFO.
        gallery_id: i64,
        /// Unexpected state found in the entry list.
        actual: DownloadState,
    },
    /// An entry was marked waiting but absent from the waiting FIFO.
    OrphanWaiting(i64),
    /// A range command repeated one gallery identifier.
    DuplicateCommandGalleryId(i64),
    /// A single-task start command referenced no snapshot entry.
    MissingStartEntry(i64),
    /// A finish callback did not identify the active task.
    FinishNotCurrent {
        /// Gallery identifier reported by the callback.
        gallery_id: i64,
        /// Active task in the validated snapshot.
        current: Option<i64>,
    },
}

impl fmt::Display for DownloadQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveGalleryId(gallery_id) => {
                write!(
                    formatter,
                    "gallery identifier must be positive: {gallery_id}"
                )
            }
            Self::DuplicateEntry(gallery_id) => {
                write!(formatter, "duplicate download entry: {gallery_id}")
            }
            Self::MissingCurrent(gallery_id) => {
                write!(formatter, "active download entry is missing: {gallery_id}")
            }
            Self::CurrentState { gallery_id, actual } => write!(
                formatter,
                "active download {gallery_id} has state {actual:?} instead of Downloading"
            ),
            Self::UnexpectedDownloading(gallery_id) => {
                write!(formatter, "download {gallery_id} is active outside current")
            }
            Self::DuplicateWaiting(gallery_id) => {
                write!(formatter, "duplicate waiting download: {gallery_id}")
            }
            Self::MissingWaiting(gallery_id) => {
                write!(formatter, "waiting download entry is missing: {gallery_id}")
            }
            Self::WaitingState { gallery_id, actual } => write!(
                formatter,
                "waiting download {gallery_id} has state {actual:?} instead of Waiting"
            ),
            Self::OrphanWaiting(gallery_id) => {
                write!(
                    formatter,
                    "waiting download is absent from FIFO: {gallery_id}"
                )
            }
            Self::DuplicateCommandGalleryId(gallery_id) => {
                write!(
                    formatter,
                    "duplicate command gallery identifier: {gallery_id}"
                )
            }
            Self::MissingStartEntry(gallery_id) => {
                write!(formatter, "download to start is missing: {gallery_id}")
            }
            Self::FinishNotCurrent {
                gallery_id,
                current,
            } => write!(
                formatter,
                "finished download {gallery_id} does not match active download {current:?}"
            ),
        }
    }
}

impl std::error::Error for DownloadQueueError {}

/// Validates and applies one command to a complete download scheduler snapshot.
///
/// Start commands automatically advance an idle FIFO. Stopping or finishing the
/// active task also advances the next waiting task, except [`DownloadCommand::StopAll`].
/// Missing identifiers in range/stop commands are ignored to preserve the existing
/// Android bulk-action behavior; a single start must reference an entry that
/// Android has already created. Duplicate range identifiers are rejected instead
/// of reproducing the legacy duplicate-FIFO bug. A finish command must use the
/// active identifier captured by Android; progress counters remain Android-owned.
///
/// # Errors
///
/// Returns [`DownloadQueueError`] when the input snapshot violates single-worker
/// or FIFO invariants, when identifiers are non-positive/duplicated, or when a
/// finish callback does not identify the active task.
pub fn reduce_download_queue(
    snapshot: DownloadQueueSnapshot,
    command: DownloadCommand,
) -> Result<DownloadQueuePlan, DownloadQueueError> {
    validate_snapshot(&snapshot)?;
    let previous_entries = snapshot.entries.clone();
    let mut queue = MutableDownloadQueue {
        entries: snapshot.entries,
        waiting: snapshot.waiting.into(),
        current: snapshot.current,
    };

    let (start_gallery_id, cancel_gallery_id) = match command {
        DownloadCommand::StartOne { gallery_id } => {
            validate_gallery_id(gallery_id)?;
            queue.start_one(gallery_id)?;
            (queue.activate_next()?, None)
        }
        DownloadCommand::StartRange { gallery_ids } => {
            validate_command_gallery_ids(&gallery_ids)?;
            queue.start_range(&gallery_ids);
            (queue.activate_next()?, None)
        }
        DownloadCommand::StartAll => {
            queue.start_all();
            (queue.activate_next()?, None)
        }
        DownloadCommand::ActivateNext => (queue.activate_next()?, None),
        DownloadCommand::StopOne { gallery_id } => {
            validate_gallery_id(gallery_id)?;
            let (stopped, cancel_gallery_id) = queue.stop_one(gallery_id);
            let start_gallery_id = if stopped {
                queue.activate_next()?
            } else {
                None
            };
            (start_gallery_id, cancel_gallery_id)
        }
        DownloadCommand::StopRange { gallery_ids } => {
            validate_command_gallery_ids(&gallery_ids)?;
            let (stopped, cancel_gallery_id) = queue.stop_range(&gallery_ids);
            let start_gallery_id = if stopped {
                queue.activate_next()?
            } else {
                None
            };
            (start_gallery_id, cancel_gallery_id)
        }
        DownloadCommand::StopAll => (None, queue.stop_all()),
        DownloadCommand::Finish { gallery_id, failed } => {
            validate_gallery_id(gallery_id)?;
            queue.finish(gallery_id, failed)?;
            (queue.activate_next()?, None)
        }
    };

    let snapshot = queue.into_snapshot();
    validate_snapshot(&snapshot)?;
    let changes = state_changes(previous_entries, &snapshot.entries);

    Ok(DownloadQueuePlan {
        snapshot,
        changes,
        start_gallery_id,
        cancel_gallery_id,
    })
}

/// Normalizes persisted entries before rebuilding an in-memory Android queue.
///
/// Waiting and downloading states become [`DownloadState::None`] because neither
/// the FIFO nor its worker survives Android process death. Entry order and
/// terminal/invalid states are preserved; the recovered queue is idle and empty.
///
/// # Errors
///
/// Returns [`DownloadQueueError`] when an entry identifier is non-positive or
/// duplicated.
pub fn recover_download_queue(
    mut entries: Vec<DownloadEntry>,
) -> Result<DownloadQueuePlan, DownloadQueueError> {
    entry_states(&entries)?;
    let previous_entries = entries.clone();
    for entry in &mut entries {
        entry.state = entry.state.after_process_restart();
    }
    let snapshot = DownloadQueueSnapshot {
        entries,
        waiting: Vec::new(),
        current: None,
    };
    validate_snapshot(&snapshot)?;
    let changes = state_changes(previous_entries, &snapshot.entries);
    Ok(DownloadQueuePlan {
        snapshot,
        changes,
        start_gallery_id: None,
        cancel_gallery_id: None,
    })
}

fn state_changes(
    previous_entries: Vec<DownloadEntry>,
    current_entries: &[DownloadEntry],
) -> Vec<DownloadStateChange> {
    previous_entries
        .into_iter()
        .zip(current_entries)
        .filter_map(|(previous, current)| {
            (previous.state != current.state).then_some(DownloadStateChange {
                gallery_id: current.gallery_id,
                previous: previous.state,
                current: current.state,
            })
        })
        .collect()
}

fn validate_gallery_id(gallery_id: i64) -> Result<(), DownloadQueueError> {
    if gallery_id > 0 {
        Ok(())
    } else {
        Err(DownloadQueueError::NonPositiveGalleryId(gallery_id))
    }
}

fn validate_command_gallery_ids(gallery_ids: &[i64]) -> Result<(), DownloadQueueError> {
    let mut seen = HashSet::with_capacity(gallery_ids.len());
    for &gallery_id in gallery_ids {
        validate_gallery_id(gallery_id)?;
        if !seen.insert(gallery_id) {
            return Err(DownloadQueueError::DuplicateCommandGalleryId(gallery_id));
        }
    }
    Ok(())
}

fn validate_snapshot(snapshot: &DownloadQueueSnapshot) -> Result<(), DownloadQueueError> {
    let states = entry_states(&snapshot.entries)?;

    if let Some(current) = snapshot.current {
        validate_gallery_id(current)?;
        let Some(&state) = states.get(&current) else {
            return Err(DownloadQueueError::MissingCurrent(current));
        };
        if state != DownloadState::Downloading {
            return Err(DownloadQueueError::CurrentState {
                gallery_id: current,
                actual: state,
            });
        }
    }
    for entry in &snapshot.entries {
        if entry.state == DownloadState::Downloading && snapshot.current != Some(entry.gallery_id) {
            return Err(DownloadQueueError::UnexpectedDownloading(entry.gallery_id));
        }
    }

    let mut waiting = HashSet::with_capacity(snapshot.waiting.len());
    for &gallery_id in &snapshot.waiting {
        validate_gallery_id(gallery_id)?;
        if !waiting.insert(gallery_id) {
            return Err(DownloadQueueError::DuplicateWaiting(gallery_id));
        }
        let Some(&state) = states.get(&gallery_id) else {
            return Err(DownloadQueueError::MissingWaiting(gallery_id));
        };
        if state != DownloadState::Waiting {
            return Err(DownloadQueueError::WaitingState {
                gallery_id,
                actual: state,
            });
        }
    }
    for entry in &snapshot.entries {
        if entry.state == DownloadState::Waiting && !waiting.contains(&entry.gallery_id) {
            return Err(DownloadQueueError::OrphanWaiting(entry.gallery_id));
        }
    }
    Ok(())
}

fn entry_states(
    entries: &[DownloadEntry],
) -> Result<HashMap<i64, DownloadState>, DownloadQueueError> {
    let mut states = HashMap::with_capacity(entries.len());
    for entry in entries {
        validate_gallery_id(entry.gallery_id)?;
        if states.insert(entry.gallery_id, entry.state).is_some() {
            return Err(DownloadQueueError::DuplicateEntry(entry.gallery_id));
        }
    }
    Ok(states)
}

struct MutableDownloadQueue {
    entries: Vec<DownloadEntry>,
    waiting: VecDeque<i64>,
    current: Option<i64>,
}

impl MutableDownloadQueue {
    fn entry_mut(&mut self, gallery_id: i64) -> Option<&mut DownloadEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.gallery_id == gallery_id)
    }

    fn start_one(&mut self, gallery_id: i64) -> Result<(), DownloadQueueError> {
        if self.current == Some(gallery_id) {
            return Ok(());
        }
        let Some(entry) = self.entry_mut(gallery_id) else {
            return Err(DownloadQueueError::MissingStartEntry(gallery_id));
        };
        if entry.state != DownloadState::Waiting {
            entry.state = DownloadState::Waiting;
            self.waiting.push_back(gallery_id);
        }
        Ok(())
    }

    fn start_range(&mut self, gallery_ids: &[i64]) {
        for &gallery_id in gallery_ids {
            let Some(entry) = self.entry_mut(gallery_id) else {
                continue;
            };
            if entry.state.can_start_range() {
                entry.state = DownloadState::Waiting;
                self.waiting.push_back(gallery_id);
            }
        }
    }

    fn start_all(&mut self) {
        let gallery_ids = self
            .entries
            .iter()
            .filter(|entry| entry.state.can_start_all())
            .map(|entry| entry.gallery_id)
            .collect::<Vec<_>>();
        for gallery_id in gallery_ids {
            if let Some(entry) = self.entry_mut(gallery_id) {
                entry.state = DownloadState::Waiting;
                self.waiting.push_back(gallery_id);
            }
        }
    }

    fn stop_one(&mut self, gallery_id: i64) -> (bool, Option<i64>) {
        if self.current == Some(gallery_id) {
            self.current = None;
            if let Some(entry) = self.entry_mut(gallery_id) {
                entry.state = DownloadState::None;
            }
            return (true, Some(gallery_id));
        }
        let waiting_position = self
            .waiting
            .iter()
            .position(|&waiting| waiting == gallery_id);
        if let Some(position) = waiting_position {
            self.waiting.remove(position);
            if let Some(entry) = self.entry_mut(gallery_id) {
                entry.state = DownloadState::None;
            }
            return (true, None);
        }
        (false, None)
    }

    fn stop_all(&mut self) -> Option<i64> {
        for gallery_id in self.waiting.drain(..).collect::<Vec<_>>() {
            if let Some(entry) = self.entry_mut(gallery_id) {
                entry.state = DownloadState::None;
            }
        }
        let cancelled = self.current.take();
        if let Some(gallery_id) = cancelled
            && let Some(entry) = self.entry_mut(gallery_id)
        {
            entry.state = DownloadState::None;
        }
        cancelled
    }

    fn stop_range(&mut self, gallery_ids: &[i64]) -> (bool, Option<i64>) {
        let selected = gallery_ids.iter().copied().collect::<HashSet<_>>();
        let cancel_gallery_id = self.current.filter(|current| selected.contains(current));
        if let Some(gallery_id) = cancel_gallery_id {
            self.current = None;
            if let Some(entry) = self.entry_mut(gallery_id) {
                entry.state = DownloadState::None;
            }
        }

        let waiting_to_stop = self
            .waiting
            .iter()
            .copied()
            .filter(|gallery_id| selected.contains(gallery_id))
            .collect::<Vec<_>>();
        self.waiting
            .retain(|gallery_id| !selected.contains(gallery_id));
        for gallery_id in &waiting_to_stop {
            if let Some(entry) = self.entry_mut(*gallery_id) {
                entry.state = DownloadState::None;
            }
        }
        (
            cancel_gallery_id.is_some() || !waiting_to_stop.is_empty(),
            cancel_gallery_id,
        )
    }

    fn finish(&mut self, gallery_id: i64, failed: bool) -> Result<(), DownloadQueueError> {
        if self.current != Some(gallery_id) {
            return Err(DownloadQueueError::FinishNotCurrent {
                gallery_id,
                current: self.current,
            });
        }
        self.current = None;
        if let Some(entry) = self.entry_mut(gallery_id) {
            entry.state = if failed {
                DownloadState::Failed
            } else {
                DownloadState::Finished
            };
        }
        Ok(())
    }

    fn activate_next(&mut self) -> Result<Option<i64>, DownloadQueueError> {
        if self.current.is_some() {
            return Ok(None);
        }
        let Some(gallery_id) = self.waiting.pop_front() else {
            return Ok(None);
        };
        let Some(entry) = self.entry_mut(gallery_id) else {
            return Err(DownloadQueueError::MissingWaiting(gallery_id));
        };
        if entry.state != DownloadState::Waiting {
            return Err(DownloadQueueError::WaitingState {
                gallery_id,
                actual: entry.state,
            });
        }
        entry.state = DownloadState::Downloading;
        self.current = Some(gallery_id);
        Ok(Some(gallery_id))
    }

    fn into_snapshot(self) -> DownloadQueueSnapshot {
        DownloadQueueSnapshot {
            entries: self.entries,
            waiting: self.waiting.into(),
            current: self.current,
        }
    }
}

#[cfg(test)]
mod tests;
