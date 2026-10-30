//! Label equality, membership and list-slot plans. Room owns IDs, SQL and effects.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Maximum encoded request and response.
pub const MAX_BYTES: i32 = 8 * 1024 * 1024;

/// Phase-specific label operation; callers must take fresh snapshots after suspensions.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// First exact matching non-null name or minus one.
    Lookup,
    /// Append at list length, or minus one for null/existing names.
    Add,
    /// Null or existing label can be assigned.
    Assign,
    /// All download slots whose labels match the supplied name.
    Matching,
    /// New positions for the list suffix after a deleted slot.
    DeletePositions,
    /// User-edit validation: blank, reserved localized name, then duplicate.
    ValidateName,
    /// Initial selection in the two-prefix default-label settings menu.
    DefaultSelection,
    /// Map a settings choice to ask/default/named-label intent.
    DefaultChoice,
}

/// Nullable Java label stored as a CBOR byte string.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LabelName {
    /// Exact UTF-16LE code units, or the default unnamed group.
    pub value: Option<ByteBuf>,
}

/// Raw UTF-16 label facts or signed positions for one effect phase.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    /// Must equal one.
    pub version: u8,
    /// Phase to plan.
    pub action: Action,
    /// Source-aligned nullable labels; empty for `DeletePositions`.
    pub names: Vec<LabelName>,
    /// Exact target name, never normalized.
    pub name: Option<ByteBuf>,
    /// Source-aligned positions; only for `DeletePositions`.
    pub positions: Vec<i32>,
    /// Original removed slot; zero for other operations.
    pub index: i32,
    /// Platform blank-text observation, only for name validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blank: Option<bool>,
    /// Localized reserved default label, only for name validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved: Option<ByteBuf>,
    /// Whether a default is configured, only for initial settings selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_default: Option<bool>,
}

/// Scalar result, selected slots and corresponding positions.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Lookup/append index or assignment permission (zero/one).
    pub value: i32,
    /// Stable source slots for matching or suffix changes.
    pub indices: Vec<i32>,
    /// New position for each suffix slot, with Java wrapping subtraction.
    pub positions: Vec<i32>,
}

/// Computes a complete bounded plan without mutation or database access.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    if !valid_request(request) {
        return ApiEnvelope::err(
            1,
            "labelRules",
            RustError::Error("Invalid label rules input".into()),
        );
    }
    let found = request
        .names
        .iter()
        .zip(0_i32..)
        .find(|(name, _)| name.value.is_some() && name.value == request.name)
        .map_or(-1, |(_, index)| index);
    let mut plan = Plan {
        value: 0,
        indices: Vec::new(),
        positions: Vec::new(),
    };
    match request.action {
        Action::Lookup => plan.value = found,
        Action::Add => {
            plan.value = if request.name.is_some() && found == -1 {
                i32::try_from(request.names.len()).unwrap_or(-1)
            } else {
                -1
            }
        }
        Action::Assign => plan.value = i32::from(request.name.is_none() || found != -1),
        Action::Matching => {
            plan.indices = request
                .names
                .iter()
                .zip(0_i32..)
                .filter(|(name, _)| name.value == request.name)
                .map(|(_, index)| index)
                .collect();
        }
        Action::DeletePositions => {
            match crate::list_order::delete_positions(&request.positions, request.index) {
                Ok(suffix) => {
                    plan.indices = suffix.indices;
                    plan.positions = suffix.positions;
                }
                Err(message) => {
                    return ApiEnvelope::err(1, "labelRules", RustError::Error(message.into()));
                }
            }
        }
        Action::ValidateName => {
            plan.value = if request.blank == Some(true) {
                1
            } else if request.name == request.reserved {
                2
            } else if found >= 0 {
                3
            } else {
                0
            }
        }
        Action::DefaultSelection => {
            plan.value = if request.has_default == Some(false) {
                0
            } else {
                found + 2
            }
        }
        Action::DefaultChoice => plan.value = request.index - 2,
    }
    ApiEnvelope::ok(1, "labelRules", plan)
}

fn valid_request(request: &Request) -> bool {
    let texts = request
        .names
        .iter()
        .map(|name| &name.value)
        .chain(std::iter::once(&request.name))
        .chain(std::iter::once(&request.reserved))
        .flatten();
    let no_ui =
        request.blank.is_none() && request.reserved.is_none() && request.has_default.is_none();
    let valid_shape = match request.action {
        Action::DeletePositions => {
            no_ui
                && request.names.is_empty()
                && request.name.is_none()
                && usize::try_from(request.index).is_ok_and(|index| index < request.positions.len())
        }
        Action::ValidateName => {
            request.blank.is_some()
                && request.name.is_some()
                && request.reserved.is_some()
                && request.has_default.is_none()
                && request.index == 0
                && request.positions.is_empty()
        }
        Action::DefaultSelection => {
            request.has_default.is_some()
                && request.blank.is_none()
                && request.reserved.is_none()
                && request.index == 0
                && request.positions.is_empty()
        }
        Action::DefaultChoice => {
            no_ui
                && request.name.is_none()
                && request.positions.is_empty()
                && usize::try_from(request.index).is_ok_and(|i| i <= request.names.len() + 1)
        }
        _ => no_ui && request.positions.is_empty() && request.index == 0,
    };
    request.version == 1
        && valid_shape
        && request.names.len() <= 100_000
        && request.positions.len() <= 100_000
        && texts.clone().all(|v| v.len() <= 8192 && v.len() % 2 == 0)
        && texts.map(|v| v.len()).sum::<usize>() <= 4 * 1024 * 1024
}

#[cfg(test)]
mod tests;
