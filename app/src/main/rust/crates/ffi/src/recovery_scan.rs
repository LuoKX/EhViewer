//! Owned scan handles. No Android objects, paths, clocks or effects are retained.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use eh_domain::recovery_scan::{ScanOptions, ScanState};

const MAX_SCANS: usize = 4;

#[derive(Default)]
struct Registry {
    scans: HashMap<i64, ScanState>,
    next: i64,
}

impl Registry {
    fn create(&mut self, options: ScanOptions) -> Result<i64, &'static str> {
        if self.scans.len() >= MAX_SCANS {
            return Err("Too many active recovery scans");
        }
        let state = ScanState::new(options)?;
        let handle = self
            .next
            .checked_add(1)
            .ok_or("Recovery scan handles exhausted")?;
        self.next = handle;
        self.scans.insert(handle, state);
        Ok(handle)
    }

    fn observe(&mut self, handle: i64, id: &[u8]) -> Result<bool, &'static str> {
        self.scans
            .get_mut(&handle)
            .ok_or("Unknown recovery scan handle")?
            .observe_document(id)
    }

    fn step(&mut self, handle: i64, operation: i32, argument: i64) -> Result<i64, &'static str> {
        if operation == 6 {
            if argument != 0 {
                return Err("Invalid recovery close argument");
            }
            self.scans
                .remove(&handle)
                .ok_or("Unknown recovery scan handle")?;
            return Ok(0);
        }
        let state = self
            .scans
            .get_mut(&handle)
            .ok_or("Unknown recovery scan handle")?;
        let ticket = |value: Option<eh_domain::recovery_scan::ScanTicket>| {
            value.map_or(-1, eh_domain::recovery_scan::ScanTicket::packed)
        };
        match operation {
            1 => state.poll(argument).map(ticket),
            2 => state.gate(argument).map(i64::from),
            3 if (0..=3).contains(&argument) => state
                .accept(argument & 1 != 0, argument & 2 != 0)
                .map(ticket),
            4 if argument == 0 => {
                state.finish_directory()?;
                Ok(0)
            }
            5 if argument == 0 => Ok(i64::from(state.limit_exceeded())),
            7 if argument == 0 => {
                state.stop();
                Ok(0)
            }
            _ => Err("Invalid recovery scan operation"),
        }
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

pub(crate) fn create(options: ScanOptions) -> Result<i64, &'static str> {
    registry()
        .lock()
        .map_err(|_| "Recovery scan registry unavailable")?
        .create(options)
}

pub(crate) fn observe(handle: i64, id: &[u8]) -> Result<bool, &'static str> {
    registry()
        .lock()
        .map_err(|_| "Recovery scan registry unavailable")?
        .observe(handle, id)
}

pub(crate) fn step(handle: i64, operation: i32, argument: i64) -> Result<i64, &'static str> {
    // Closing must release retained identities even after a caught Rust panic poisoned the lock.
    if operation == 6 {
        return registry()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .step(handle, operation, argument);
    }
    registry()
        .lock()
        .map_err(|_| "Recovery scan registry unavailable")?
        .step(handle, operation, argument)
}

#[cfg(test)]
mod tests;
