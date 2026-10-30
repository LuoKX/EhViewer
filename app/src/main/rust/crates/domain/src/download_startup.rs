//! Read-only projections of persisted download states after process restart.
use crate::download::DownloadState;

/// Maximum number of rows in one projection.
pub const MAX_STATES: usize = 100_000;

/// Applies the queue's canonical restart rule while preserving unknown raw states.
#[must_use]
pub const fn normalize_state(raw: i32) -> i32 {
    let state = match raw {
        -1 => DownloadState::Invalid,
        0 => DownloadState::None,
        1 => DownloadState::Waiting,
        2 => DownloadState::Downloading,
        3 => DownloadState::Finished,
        4 => DownloadState::Failed,
        _ => return raw,
    };
    match state.after_process_restart() {
        DownloadState::None => 0,
        // All other states retain their original database encoding.
        _ => raw,
    }
}

/// Projects a complete bounded snapshot without changing persistence or live workers.
///
/// # Errors
/// Rejects oversized snapshots before producing a partial result.
pub fn normalize_states(states: &[i32]) -> Result<Vec<i32>, &'static str> {
    if states.len() > MAX_STATES {
        return Err("Download state snapshot exceeds limit");
    }
    Ok(states.iter().copied().map(normalize_state).collect())
}

#[cfg(test)]
mod tests;
