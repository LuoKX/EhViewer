//! Search controls and cursor transitions; text remains opaque Java UTF-16LE.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Encoded request limit for four bounded opaque text values.
pub const MAX_REQUEST_BYTES: i32 = 128 * 1024;
/// Maximum bytes in each cursor, seek value or saved search name.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

/// Cursor fields shared by gallery and favorites lists.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Navigation {
    /// Previous cursor, including empty strings.
    pub previous: Option<ByteBuf>,
    /// Next cursor, including empty strings.
    pub next: Option<ByteBuf>,
    /// Date or zero-based page string.
    pub seek: Option<ByteBuf>,
    /// Random range selector.
    pub range: i32,
}

/// State transition, independent of Android Paging and Compose.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Transition {
    /// Replace one cursor and clear range/seek.
    Index,
    /// Clear cursors and seek while replacing range.
    Range,
    /// Set next cursor to two and preserve the supplied date verbatim.
    Seek,
    /// Replace only seek, using Java Int wrapping arithmetic.
    Page,
    /// Restore a quick search's final at-sign suffix as next cursor.
    SavedProgress,
}

/// Bounded V1 transition input.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    /// Must equal one.
    pub version: u8,
    /// Complete prior navigation state.
    pub state: Navigation,
    /// Requested transition.
    pub action: Transition,
    /// Cursor/date/saved name; absent for numeric actions.
    pub text: Option<ByteBuf>,
    /// Numeric page or range, zero for other actions.
    pub value: i32,
    /// Direction for Index; false for other actions.
    pub forward: bool,
}

/// Applies an entire validated transition with no URL building or side effects.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Navigation> {
    let texts = [
        &request.state.previous,
        &request.state.next,
        &request.state.seek,
        &request.text,
    ];
    let shape_ok = match request.action {
        Transition::Index => request.value == 0,
        Transition::Range | Transition::Page => request.text.is_none() && !request.forward,
        Transition::Seek | Transition::SavedProgress => {
            request.text.is_some() && request.value == 0 && !request.forward
        }
    };
    if request.version != 1
        || !shape_ok
        || texts
            .iter()
            .filter_map(|v| v.as_ref())
            .any(|v| v.len() > MAX_TEXT_BYTES || v.len() % 2 != 0)
    {
        return ApiEnvelope::err(
            1,
            "catalogState",
            RustError::Error("Invalid catalog navigation input".into()),
        );
    }
    let mut state = request.state.clone();
    match request.action {
        Transition::Index => {
            state.next = request.text.clone().filter(|_| request.forward);
            state.previous = request.text.clone().filter(|_| !request.forward);
            state.seek = None;
            state.range = 0;
        }
        Transition::Range => {
            state = Navigation {
                range: request.value,
                ..Navigation::default()
            }
        }
        Transition::Seek => {
            state = Navigation {
                next: Some(ByteBuf::from(vec![b'2', 0])),
                seek: request.text.clone(),
                ..Navigation::default()
            }
        }
        Transition::Page => {
            let page = request.value.wrapping_sub(1);
            state.seek = (page != 0).then(|| {
                ByteBuf::from(
                    page.to_string()
                        .encode_utf16()
                        .flat_map(u16::to_le_bytes)
                        .collect::<Vec<_>>(),
                )
            });
        }
        Transition::SavedProgress => {
            state.next = request.text.as_ref().and_then(|text| {
                text.chunks_exact(2)
                    .rposition(|unit| unit == [b'@', 0])
                    .map(|index| ByteBuf::from(text[(index + 1) * 2..].to_vec()))
                    .filter(|suffix| !suffix.is_empty())
            });
        }
    }
    ApiEnvelope::ok(1, "catalogState", state)
}

/// Numeric search controls and toplist bounds. Operation codes are fixed by V1.
/// Returns one scalar except op 7 (presence/value pairs for previous/next).
///
/// # Errors
/// Rejects unknown operations.
pub fn numbers(operation: i32, left: i32, right: i32) -> Result<Vec<i32>, &'static str> {
    let value = match operation {
        0 => left ^ right,
        1 => left.wrapping_sub(1).max(0),
        2 => {
            if left == 0 {
                0
            } else {
                left.wrapping_add(1)
            }
        }
        3 => left.clamp(0, 1000),
        4 => left.clamp(0, 2000),
        5 => {
            if right == 0 {
                0
            } else if left == 0 {
                i32::from(right < 10)
            } else {
                2 * i32::from(left > (right / 2).min(right.wrapping_sub(20)))
            }
        }
        6 => i32::from((1..=200).contains(&left)),
        7 => {
            let previous = left.wrapping_sub(1);
            let next = left.wrapping_add(1);
            return Ok(vec![
                i32::from(previous > 0),
                previous,
                i32::from(next <= 200),
                next,
            ]);
        }
        8 => left.wrapping_add(1),
        9 => 200,
        10 => i32::from(crate::quick_search::creation_allowed(left)),
        _ => return Err("Unknown catalog numeric operation"),
    };
    Ok(vec![value])
}

#[cfg(test)]
mod tests;
