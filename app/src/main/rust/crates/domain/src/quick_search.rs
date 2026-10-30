//! Quick-search creation and batched duplicate checks with opaque Java names.
//! Android supplies character classifications from its own Unicode implementation.
use crate::{
    contracts::{ApiEnvelope, RustError},
    library::{QuickSearchSnapshot, quick_search_matches},
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::collections::{HashMap, HashSet};

/// Maximum encoded request/response size.
pub const MAX_BYTES: i32 = 16 * 1024 * 1024;
/// Maximum raw name bytes, including any generated progress suffix.
pub const MAX_NAME_BYTES: usize = 32 * 1024;
/// Maximum saved rows per snapshot.
pub const MAX_ITEMS: usize = 100_000;

/// Pure creation eligibility, before the UI reads its first visible gallery.
#[must_use]
pub const fn creation_allowed(mode: i32) -> bool {
    mode != 4
}

/// Exact non-ASCII decimal classification observed through JVM Character.digit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Digit {
    /// Java character code unit.
    pub unit: u16,
    /// Its decimal value, zero through nine.
    pub value: i32,
}

/// Query fields retain the existing library wire contract; names retain raw code units.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Saved {
    /// Original query fields.
    pub query: QuickSearchSnapshot,
    /// Exact persisted Java name.
    pub name: ByteBuf,
}

/// Byte-string wrapper for a saved name in a CBOR collection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Name {
    /// Exact Java code units encoded as UTF-16LE.
    pub value: ByteBuf,
}

/// Two phases separated by the Android-owned dialog.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Compute next progress and first duplicate query/progress pair.
    Check,
    /// Validate and construct a name against the latest names snapshot.
    Create,
}

/// Bounded phase-specific input; irrelevant fields must be absent/empty.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    /// Must equal one.
    pub version: u8,
    /// Requested phase.
    pub kind: Kind,
    /// Query to compare, only for Check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<QuickSearchSnapshot>,
    /// Source-ordered saved rows, only for Check.
    pub saved: Vec<Saved>,
    /// First visible gallery ID, only for Check.
    pub first_gid: Option<i64>,
    /// Non-ASCII decimal facts, only for Check.
    pub digits: Vec<Digit>,
    /// Raw editable name, only for Create.
    pub input: Option<ByteBuf>,
    /// Observed whitespace code units, only for Create.
    pub whitespace: Vec<u16>,
    /// Whether to append progress, only for Create.
    pub save_progress: Option<bool>,
    /// Captured progress from Check, only for Create.
    pub next: Option<i64>,
    /// Latest exact names, only for Create.
    pub names: Vec<Name>,
}

/// Localized messages are selected by Android from these fixed codes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NameError {
    /// Input becomes empty after the existing whitespace trim.
    Empty,
    /// Final name including suffix already exists.
    Duplicate,
}

/// Complete atomic result for one phase.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Echoed phase.
    pub kind: Kind,
    /// Captured wrapping next gallery ID, only for Check.
    pub next: Option<i64>,
    /// First duplicate saved row, only for Check.
    pub duplicate: Option<usize>,
    /// Final raw name, only for successful Create.
    pub name: Option<ByteBuf>,
    /// Append position, only for successful Create.
    pub position: Option<i32>,
    /// Expected user-input error, only for Create.
    pub error: Option<NameError>,
}

/// Plans without reading platform state or mutating any row.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, "quickSearchEdit", plan),
        Err(message) => ApiEnvelope::err(1, "quickSearchEdit", RustError::Error(message.into())),
    }
}

fn plan(r: &Request) -> Result<Plan, &'static str> {
    let invalid = "Invalid quick search edit input";
    validate(r)?;
    let mut result = Plan {
        kind: r.kind,
        next: None,
        duplicate: None,
        name: None,
        position: None,
        error: None,
    };
    match r.kind {
        Kind::Check => {
            let (Some(current), Some(gid)) = (&r.current, r.first_gid) else {
                return Err(invalid);
            };
            if !creation_allowed(current.mode)
                || r.input.is_some()
                || !r.whitespace.is_empty()
                || r.save_progress.is_some()
                || r.next.is_some()
                || !r.names.is_empty()
            {
                return Err(invalid);
            }
            let digits: HashMap<u16, i32> = r.digits.iter().map(|d| (d.unit, d.value)).collect();
            if digits.len() != r.digits.len()
                || r.digits
                    .iter()
                    .any(|d| d.unit < 128 || !(0..=9).contains(&d.value))
            {
                return Err(invalid);
            }
            let next = gid.wrapping_add(1);
            result.next = Some(next);
            result.duplicate = r.saved.iter().position(|row| {
                quick_search_matches(current, Some(&row.query))
                    && parse_progress(&row.name, &digits) == Some(next)
            });
        }
        Kind::Create => {
            let (Some(input), Some(save), Some(next)) = (&r.input, r.save_progress, r.next) else {
                return Err(invalid);
            };
            if r.current.is_some()
                || !r.saved.is_empty()
                || r.first_gid.is_some()
                || !r.digits.is_empty()
            {
                return Err(invalid);
            }
            let whitespace: HashSet<u16> = r.whitespace.iter().copied().collect();
            if whitespace.len() != r.whitespace.len() {
                return Err(invalid);
            }
            let units = units(input);
            let start = units
                .iter()
                .position(|u| !whitespace.contains(u))
                .unwrap_or(units.len());
            let end = units
                .iter()
                .rposition(|u| !whitespace.contains(u))
                .map_or(start, |i| i + 1);
            if start == end {
                result.error = Some(NameError::Empty);
                return Ok(result);
            }
            let mut name = input[start * 2..end * 2].to_vec();
            if save {
                name.extend(format!("@{next}").encode_utf16().flat_map(u16::to_le_bytes));
            }
            if name.len() > MAX_NAME_BYTES {
                return Err(invalid);
            }
            if r.names.iter().any(|old| old.value.as_ref() == name) {
                result.error = Some(NameError::Duplicate);
            } else {
                result.name = Some(ByteBuf::from(name));
                result.position = Some(i32::try_from(r.names.len()).map_err(|_| invalid)?);
            }
        }
    }
    Ok(result)
}

fn validate(r: &Request) -> Result<(), &'static str> {
    let invalid = "Invalid quick search edit input";
    let names = r
        .saved
        .iter()
        .map(|row| &row.name)
        .chain(r.names.iter().map(|n| &n.value))
        .chain(r.input.iter());
    let queries = r.saved.iter().map(|row| &row.query).chain(r.current.iter());
    if r.version != 1
        || r.saved.len() > MAX_ITEMS
        || r.names.len() > MAX_ITEMS
        || r.digits.len() > 65_536
        || r.whitespace.len() > 65_536
        || names
            .clone()
            .any(|name| name.len() > MAX_NAME_BYTES || name.len() % 2 != 0)
        || names.map(|name| name.len()).sum::<usize>() > 8 * 1024 * 1024
        || queries
            .filter_map(|q| q.keyword.as_ref())
            .map(String::len)
            .sum::<usize>()
            > 4 * 1024 * 1024
    {
        return Err(invalid);
    }
    Ok(())
}

fn units(raw: &[u8]) -> Vec<u16> {
    raw.chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect()
}

fn parse_progress(raw: &[u8], digits: &HashMap<u16, i32>) -> Option<i64> {
    let units = units(raw);
    let offset = units.iter().rposition(|&u| u == u16::from(b'@'))? + 1;
    let text = &units[offset..];
    let first = *text.first()?;
    let negative = first == u16::from(b'-');
    let sign = usize::from(negative || first == u16::from(b'+'));
    if text.len() == sign {
        return None;
    }
    let limit = if negative { i64::MIN } else { -i64::MAX };
    let mut number = 0_i64;
    for &unit in &text[sign..] {
        let digit = if (48..=57).contains(&unit) {
            i32::from(unit - 48)
        } else {
            *digits.get(&unit)?
        };
        number = number.checked_mul(10)?.checked_sub(i64::from(digit))?;
        if number < limit {
            return None;
        }
    }
    if negative {
        Some(number)
    } else {
        number.checked_neg()
    }
}

#[cfg(test)]
mod tests;
