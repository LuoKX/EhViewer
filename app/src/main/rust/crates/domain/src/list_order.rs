//! List movement and local position changes, independent of UI rows and SQL positions.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Maximum rows observed in one list.
pub const MAX_ITEMS: usize = 100_000;
/// Encoded transport budget.
pub const MAX_BYTES: i32 = 2 * 1024 * 1024;

/// Phase of a list edit.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Translate UI slots to one remove/insert pair.
    Move,
    /// Reindex only the interval between the original and final drag slots.
    Finish,
    /// Decrement the original positions of the suffix after deletion.
    Delete,
}

/// Fresh facts for a single synchronous list phase.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    /// Must equal one.
    pub version: u8,
    /// Operation to plan.
    pub action: Action,
    /// Number of current data rows, excluding UI headers.
    pub count: i32,
    /// Source slot, or minus one for an inactive Finish.
    pub from: i32,
    /// Destination slot, zero for Delete.
    pub to: i32,
    /// UI header count for Move; zero for other operations.
    pub headers: i32,
    /// Current signed positions, populated only for Delete.
    pub positions: Vec<i32>,
}

/// Complete list intent; callers validate it before touching any row.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Move source, otherwise absent.
    pub from: Option<i32>,
    /// Move destination, otherwise absent.
    pub to: Option<i32>,
    /// Stable source slots to update.
    pub indices: Vec<i32>,
    /// Corresponding new position values.
    pub positions: Vec<i32>,
}

/// Shared suffix rule used by labels and quick searches.
///
/// # Errors
/// Rejects invalid slots or oversized snapshots before producing an intent.
pub fn delete_positions(positions: &[i32], removed: i32) -> Result<Plan, &'static str> {
    if positions.len() > MAX_ITEMS || !usize::try_from(removed).is_ok_and(|i| i < positions.len()) {
        return Err("Invalid list deletion input");
    }
    let (indices, positions) = positions
        .iter()
        .zip(0_i32..)
        .filter(|(_, i)| *i > removed)
        .map(|(position, i)| (i, position.wrapping_sub(1)))
        .unzip();
    Ok(Plan {
        from: None,
        to: None,
        indices,
        positions,
    })
}

/// Plans one move, drag completion or deletion without effects.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, "listOrder", plan),
        Err(message) => ApiEnvelope::err(1, "listOrder", RustError::Error(message.into())),
    }
}

fn plan(r: &Request) -> Result<Plan, &'static str> {
    let invalid = "Invalid list order input";
    if r.version != 1 || !usize::try_from(r.count).is_ok_and(|n| n <= MAX_ITEMS) {
        return Err(invalid);
    }
    if r.action == Action::Delete {
        if r.to != 0 || r.headers != 0 || usize::try_from(r.count).ok() != Some(r.positions.len()) {
            return Err(invalid);
        }
        return delete_positions(&r.positions, r.from);
    }
    if !r.positions.is_empty() || r.headers < 0 || (r.action == Action::Finish && r.headers != 0) {
        return Err(invalid);
    }
    let mut plan = Plan {
        from: None,
        to: None,
        indices: vec![],
        positions: vec![],
    };
    // The original drag callback ignores an inactive drag and a return to its starting slot.
    if r.action == Action::Finish && (r.from == -1 || r.from == r.to) {
        return Ok(plan);
    }
    let from = r.from.checked_sub(r.headers).ok_or(invalid)?;
    let to = r.to.checked_sub(r.headers).ok_or(invalid)?;
    if !(0..r.count).contains(&from) || !(0..r.count).contains(&to) {
        return Err(invalid);
    }
    if r.action == Action::Move {
        plan.from = Some(from);
        plan.to = Some(to);
    } else {
        plan.indices = (from.min(to)..=from.max(to)).collect();
        plan.positions.clone_from(&plan.indices);
    }
    Ok(plan)
}

#[cfg(test)]
mod tests;
