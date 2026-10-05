//! Native-owned rolling transfer measurements; callers supply monotonic nanoseconds.

use crate::contracts::RustError;
use std::collections::BTreeMap;

const MIN_WINDOW_NANOS: i64 = 10_000_000;
const MAX_SAMPLES: usize = 65_536;

/// One rolling measurement window. No Java buffer or object reference is retained.
#[derive(Debug)]
pub struct SpeedWindow {
    window_nanos: i64,
    start: Option<i64>,
    received: BTreeMap<i64, i32>,
}

impl SpeedWindow {
    /// Creates a finite window of at least ten milliseconds.
    ///
    /// # Errors
    /// Rejects shorter or invalid windows with a fixed diagnostic.
    pub fn new(window_nanos: i64) -> Result<Self, RustError> {
        if window_nanos < MIN_WINDOW_NANOS {
            return Err(invalid());
        }
        Ok(Self {
            window_nanos,
            start: None,
            received: BTreeMap::new(),
        })
    }

    /// Clears all measurements without retiring the owning handle.
    pub fn reset(&mut self) {
        self.start = None;
        self.received.clear();
    }

    /// Tracks one chunk, pruning stale samples even when the flow is not collected.
    ///
    /// Same-time chunks overwrite, matching the former Kotlin map semantics.
    ///
    /// # Errors
    /// Rejects negative bytes and sample-count exhaustion; no silent fallback.
    pub fn track(&mut self, now: i64, bytes: i32) -> Result<(), RustError> {
        if bytes < 0 {
            return Err(invalid());
        }
        self.prune(now.saturating_sub(self.window_nanos));
        if self.received.len() >= MAX_SAMPLES && !self.received.contains_key(&now) {
            return Err(invalid());
        }
        self.start.get_or_insert(now);
        self.received.insert(now, bytes);
        Ok(())
    }

    fn prune(&mut self, cutoff: i64) {
        while self
            .received
            .first_key_value()
            .is_some_and(|(time, _)| *time < cutoff)
        {
            self.received.pop_first();
        }
    }

    /// Samples bytes/second; absent until the first chunk, zero for a stalled transfer.
    ///
    /// # Errors
    /// Reports checked byte-count overflow using a fixed diagnostic.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "speed preserves legacy Double division followed by Long saturation"
    )]
    pub fn sample(&mut self, now: i64) -> Result<Option<i64>, RustError> {
        let Some(start) = self.start else {
            return Ok(None);
        };
        let window = now
            .saturating_sub(start)
            .clamp(MIN_WINDOW_NANOS, self.window_nanos);
        self.prune(now.saturating_sub(window));
        let total = self
            .received
            .values()
            .try_fold(0_i64, |sum, bytes| sum.checked_add(i64::from(*bytes)))
            .ok_or_else(invalid)?;
        Ok(Some(
            (total as f64 / (window as f64 / 1_000_000_000.0)) as i64,
        ))
    }
}

fn invalid() -> RustError {
    RustError::Error("Invalid transfer speed input".into())
}

#[cfg(test)]
mod tests;
