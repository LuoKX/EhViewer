//! Bounded BFS recovery policy. Android supplies clocks, IDs and file observations.
use std::collections::{HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Hard resource ceiling independent of caller-supplied limits.
pub const MAX_NODES: i32 = 100_000;
/// Maximum retained provider identity bytes, including depth-rejected documents.
pub const MAX_SEEN_BYTES: usize = 8 * 1024 * 1024;
/// Maximum one provider identity in raw UTF-16LE bytes.
pub const MAX_ID_BYTES: usize = 128 * 1024;

/// Platform traversal backend.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanBackend {
    /// Symlinks are skipped; platform enumerations are name-sorted.
    Physical,
    /// Provider order is retained and document IDs are deduplicated.
    Documents,
}

/// One scan configuration; signed limits retain legacy zero/negative behavior.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    /// Closed ABI version.
    pub version: u8,
    /// Platform backend.
    pub backend: ScanBackend,
    /// Maximum directory depth; loose files remain observable one level deeper.
    pub max_depth: i32,
    /// Maximum discovered nodes, including the root.
    pub max_nodes: i32,
    /// Android elapsed-realtime observation after the root progress callback.
    pub started_at_millis: i64,
    /// Duration used with Java-compatible wrapping addition.
    pub max_duration_millis: i64,
    /// Root provider ID, UTF-16LE. Empty for physical scans.
    pub root_document_id: ByteBuf,
}

/// Opaque Android node slot and Rust-computed depth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanTicket {
    /// Slot in the Android snapshot, not a platform object or pointer.
    pub index: u32,
    /// Root-relative depth.
    pub depth: u32,
}

impl ScanTicket {
    /// Compact non-negative JNI representation. `-1` is reserved for no ticket.
    #[must_use]
    pub fn packed(self) -> i64 {
        (i64::from(self.index) << 32) | i64::from(self.depth)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RowPhase {
    Idle,
    Permitted,
    Identified,
}

/// No Android objects, effects, global state or callbacks are retained here.
pub struct ScanState {
    backend: ScanBackend,
    max_depth: i32,
    max_nodes: i32,
    deadline: i64,
    discovered: u32,
    pending: VecDeque<ScanTicket>,
    active: Option<ScanTicket>,
    row: RowPhase,
    seen: HashSet<Vec<u8>>,
    seen_bytes: usize,
    stopped: bool,
    limit_exceeded: bool,
}

impl ScanState {
    /// Create one root-inclusive scan; reject configurations outside hard bounds.
    ///
    /// # Errors
    /// Returns input-only errors; document identities never appear in error text.
    pub fn new(options: ScanOptions) -> Result<Self, &'static str> {
        if options.version != 1 || options.max_nodes > MAX_NODES {
            return Err("Invalid recovery scan configuration");
        }
        let root = options.root_document_id.into_vec();
        if root.len() > MAX_ID_BYTES || !root.len().is_multiple_of(2) {
            return Err("Invalid recovery document identity");
        }
        if options.backend == ScanBackend::Physical && !root.is_empty() {
            return Err("Physical recovery scan has a provider identity");
        }
        let mut seen = HashSet::new();
        let seen_bytes = if options.backend == ScanBackend::Documents {
            let bytes = root.len();
            seen.insert(root);
            bytes
        } else {
            0
        };
        Ok(Self {
            backend: options.backend,
            max_depth: options.max_depth,
            max_nodes: options.max_nodes,
            deadline: options
                .started_at_millis
                .wrapping_add(options.max_duration_millis),
            discovered: 1,
            pending: VecDeque::from([ScanTicket { index: 0, depth: 0 }]),
            active: None,
            row: RowPhase::Idle,
            seen,
            seen_bytes,
            stopped: false,
            limit_exceeded: false,
        })
    }

    /// Pop the next BFS directory. An empty queue does not spuriously exceed a limit.
    ///
    /// # Errors
    /// Rejects polling before finishing the previous platform enumeration.
    pub fn poll(&mut self, now: i64) -> Result<Option<ScanTicket>, &'static str> {
        if self.active.is_some() {
            return Err("Recovery directory enumeration is unfinished");
        }
        if self.pending.is_empty() || self.stopped {
            return Ok(None);
        }
        if !self.budget_allows(now) {
            return Ok(None);
        }
        let ticket = self.pending.pop_front();
        self.active = ticket;
        Ok(ticket)
    }

    /// Check budgets before platform child metadata access.
    ///
    /// # Errors
    /// Rejects an unconsumed row or a missing active directory.
    pub fn gate(&mut self, now: i64) -> Result<bool, &'static str> {
        if self.active.is_none() || self.row != RowPhase::Idle {
            return Err("Invalid recovery row phase");
        }
        if !self.budget_allows(now) {
            return Ok(false);
        }
        self.row = RowPhase::Permitted;
        Ok(true)
    }

    /// Deduplicate before reading other provider columns, including rejected depths.
    ///
    /// # Errors
    /// Rejects backend/phase mismatches and malformed UTF-16LE identities.
    pub fn observe_document(&mut self, id: &[u8]) -> Result<bool, &'static str> {
        if self.backend != ScanBackend::Documents || self.row != RowPhase::Permitted {
            return Err("Invalid recovery document phase");
        }
        if !id.len().is_multiple_of(2) {
            return Err("Invalid recovery document identity");
        }
        if id.len() > MAX_ID_BYTES {
            self.stop();
            return Ok(false);
        }
        if self.seen.contains(id) {
            self.row = RowPhase::Idle;
            return Ok(false);
        }
        if self.seen.len() >= MAX_NODES as usize || id.len() > MAX_SEEN_BYTES - self.seen_bytes {
            self.stop();
            return Ok(false);
        }
        self.seen_bytes += id.len();
        self.seen.insert(id.to_vec());
        self.row = RowPhase::Identified;
        Ok(true)
    }

    /// Accept one node and decide directory enqueueing. Android materializes the ticket.
    ///
    /// # Errors
    /// Rejects protocol mismatches without consuming an unrelated row.
    pub fn accept(
        &mut self,
        directory: bool,
        symbolic_link: bool,
    ) -> Result<Option<ScanTicket>, &'static str> {
        let expected = match self.backend {
            ScanBackend::Physical => RowPhase::Permitted,
            ScanBackend::Documents => RowPhase::Identified,
        };
        if self.row != expected || (self.backend == ScanBackend::Documents && symbolic_link) {
            return Err("Invalid recovery child phase");
        }
        let parent = self.active.ok_or("Missing recovery directory")?;
        self.row = RowPhase::Idle;
        if symbolic_link {
            return Ok(None);
        }
        let depth = parent.depth + 1;
        if directory && i64::from(depth) > i64::from(self.max_depth) {
            self.limit_exceeded = true;
            return Ok(None);
        }
        let ticket = ScanTicket {
            index: self.discovered,
            depth,
        };
        self.discovered += 1;
        if directory {
            self.pending.push_back(ticket);
        }
        Ok(Some(ticket))
    }

    /// Finish a provider cursor or physical enumeration, including budget-stopped rows.
    ///
    /// # Errors
    /// Rejects a missing directory or partially consumed observation.
    pub fn finish_directory(&mut self) -> Result<(), &'static str> {
        if self.active.is_none() || self.row != RowPhase::Idle {
            return Err("Invalid recovery directory completion");
        }
        self.active = None;
        Ok(())
    }

    /// Stop on a hard platform observation ceiling. No further enqueueing is allowed.
    pub fn stop(&mut self) {
        self.stopped = true;
        self.limit_exceeded = true;
        self.row = RowPhase::Idle;
        self.pending.clear();
    }

    /// Whether a caller limit or hard resource ceiling stopped part of the scan.
    #[must_use]
    pub fn limit_exceeded(&self) -> bool {
        self.limit_exceeded
    }

    fn budget_allows(&mut self, now: i64) -> bool {
        if self.stopped
            || now >= self.deadline
            || i64::from(self.discovered) >= i64::from(self.max_nodes)
        {
            self.stop();
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests;
