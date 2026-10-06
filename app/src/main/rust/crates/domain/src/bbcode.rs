//! Bounded UTF-16 `BBCode` serialization and legacy-compatible text edit plans.
//! Text lives only for one operation; no logging, persistence or Compose objects.

use crate::contracts::{ApiEnvelope, RustError};
use crate::text_spans::TextSpan;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

mod diff;
/// Maximum source or target Java string length, measured in UTF-16 units.
pub const MAX_TEXT_UNITS: usize = 65_536;
/// Maximum observed style ranges.
pub const MAX_SPANS: usize = 100_000;
/// Maximum exported UTF-16 result, including markup.
pub const MAX_OUTPUT_UNITS: usize = 1_000_000;

/// One complete text operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BbCodeOperation {
    /// Serialize source-ordered style observations into `BBCode`.
    Export,
    /// Classify edits and return slices; Android retains opaque annotations.
    Edit,
}
/// V1 bounded observation. UTF-16 arrays preserve lone surrogates exactly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BbCodeRequest {
    /// Protocol version.
    pub version: u8,
    /// Selected operation.
    pub operation: BbCodeOperation,
    /// Source text for export, new text for edit.
    pub units: Vec<u16>,
    /// Original text, present only for edit.
    pub origin_units: Option<Vec<u16>>,
    /// Observed bold/italic/underline/strikethrough bits (1/2/4/8); Rust selects priority.
    /// Unsupported/empty ranges still participate in the legacy tag stack.
    pub spans: Vec<TextSpan>,
}
/// Deterministic adapter action, never a native fallback instruction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BbCodeAction {
    /// Return the complete exported units.
    Export,
    /// Reuse the original annotated string, preserving opaque annotations.
    Original,
    /// Keep the incoming text and annotations for complex/replacement edits or resource exhaustion.
    Target,
    /// Apply one Rust-planned insertion/deletion, retaining annotations on original slices.
    Splice,
}
/// Half-open UTF-16 slice plan. Insertions have equal source bounds; deletions have equal target bounds.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// Original prefix end and changed source start.
    pub source_start: u32,
    /// Original suffix start and changed source end.
    pub source_end: u32,
    /// New inserted slice start.
    pub target_start: u32,
    /// New inserted slice end.
    pub target_end: u32,
}
/// Complete operation-tagged result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BbCodePlan {
    /// Adapter action.
    pub action: BbCodeAction,
    /// Exported UTF-16 result, present only for export.
    pub units: Option<Vec<u16>>,
    /// Bounded slice coordinates, present only for splice.
    pub edit: Option<TextEdit>,
}

/// Execute exactly one operation with input-free errors and bounded work/allocation.
/// Single CHANGE deltas formerly crashed Kotlin; they now keep incoming text.
#[must_use]
pub fn build_envelope(request: &BbCodeRequest) -> ApiEnvelope<BbCodePlan> {
    let valid = request.version == 1
        && request.units.len() <= MAX_TEXT_UNITS
        && request.spans.len() <= MAX_SPANS
        && request
            .origin_units
            .as_ref()
            .is_none_or(|text| text.len() <= MAX_TEXT_UNITS)
        && request.spans.iter().all(|span| {
            span.start <= span.end && span.end as usize <= request.units.len() && span.flags <= 15
        });
    if !valid {
        return failure("Invalid BBCode input");
    }
    let plan = match request.operation {
        BbCodeOperation::Export => {
            if request.origin_units.is_some() {
                return failure("Invalid BBCode operation fields");
            }
            let Ok(units) = export(&request.units, &request.spans, MAX_OUTPUT_UNITS) else {
                return failure("BBCode output limit exceeded");
            };
            BbCodePlan {
                action: BbCodeAction::Export,
                units: Some(units),
                edit: None,
            }
        }
        BbCodeOperation::Edit => {
            let Some(origin) = &request.origin_units else {
                return failure("Invalid BBCode operation fields");
            };
            if !request.spans.is_empty() {
                return failure("Invalid BBCode operation fields");
            }
            match diff::classify(origin, &request.units) {
                diff::EditResult::Original => BbCodePlan {
                    action: BbCodeAction::Original,
                    units: None,
                    edit: None,
                },
                diff::EditResult::Target => BbCodePlan {
                    action: BbCodeAction::Target,
                    units: None,
                    edit: None,
                },
                diff::EditResult::Splice(edit) => BbCodePlan {
                    action: BbCodeAction::Splice,
                    units: None,
                    edit: Some(edit),
                },
            }
        }
    };
    ApiEnvelope::ok(1, "bbcode", plan)
}
fn failure(message: &str) -> ApiEnvelope<BbCodePlan> {
    ApiEnvelope::err(1, "bbcode", RustError::Error(message.into()))
}

fn export(units: &[u16], spans: &[TextSpan], limit: usize) -> Result<Vec<u16>, ()> {
    let mut order = (0..spans.len()).collect::<Vec<_>>();
    // Stable sort preserves source order for same-start/same-end observations.
    order.sort_by_key(|&index| (spans[index].start, Reverse(spans[index].end)));
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut output = Vec::new();
    for position in 0..=units.len() {
        while stack
            .last()
            .is_some_and(|&index| spans[index].end as usize == position)
        {
            let index = stack.pop().expect("checked stack");
            append(&mut output, tag(spans[index].flags, true), limit)?;
        }
        if position == units.len() {
            break;
        }
        while next < order.len() && spans[order[next]].start as usize == position {
            let index = order[next];
            next += 1;
            append(&mut output, tag(spans[index].flags, false), limit)?;
            stack.push(index);
        }
        append(&mut output, &units[position..=position], limit)?;
    }
    Ok(output)
}
fn tag(flags: u8, close: bool) -> &'static [u16] {
    let flag = [1, 2, 4, 8]
        .into_iter()
        .find(|&bit| flags & bit != 0)
        .unwrap_or(0);
    match (flag, close) {
        (1, false) => &[91, 98, 93],
        (1, true) => &[91, 47, 98, 93],
        (2, false) => &[91, 105, 93],
        (2, true) => &[91, 47, 105, 93],
        (4, false) => &[91, 117, 93],
        (4, true) => &[91, 47, 117, 93],
        (8, false) => &[91, 115, 93],
        (8, true) => &[91, 47, 115, 93],
        _ => &[],
    }
}
fn append(output: &mut Vec<u16>, units: &[u16], limit: usize) -> Result<(), ()> {
    if output
        .len()
        .checked_add(units.len())
        .is_none_or(|len| len > limit)
    {
        return Err(());
    }
    output.extend_from_slice(units);
    Ok(())
}

#[cfg(test)]
mod tests;
