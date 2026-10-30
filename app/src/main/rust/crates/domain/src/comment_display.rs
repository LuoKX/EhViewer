//! Fixed comment-link and image-URL grammar over raw Java UTF-16.
//! Platform digit facts retain the running JVM/Android Unicode behavior without
//! replacing Android's user-pattern regex engine or HTML/Span renderer.
use regex_lite::Regex;
use std::sync::LazyLock;

/// Maximum UTF-16 units per comment or image source.
pub const MAX_TEXT: usize = 128 * 1024;

/// Fixed, input-free failures for display preprocessing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayError {
    /// Text or observation budget/shape is invalid.
    Input,
    /// A source image dimension does not fit Kotlin's signed Int parser.
    Number,
}

impl DisplayError {
    /// Sanitized boundary diagnostic.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Input => "Invalid comment text observations",
            Self::Number => "Invalid comment image dimension",
        }
    }
}

fn validate(text: &[u16], digits: &[u8]) -> Result<(), DisplayError> {
    if text.len() > MAX_TEXT
        || text.len() != digits.len()
        || digits
            .iter()
            .any(|fact| fact & !31 != 0 || !matches!(fact & 15, 0..=9 | 15))
    {
        return Err(DisplayError::Input);
    }
    Ok(())
}

fn host(unit: u16) -> bool {
    matches!(unit, 37 | 45 | 48..=57 | 65..=90 | 97..=122)
}
fn path(unit: u16) -> bool {
    host(unit)
        || matches!(
            unit,
            95 | 126
                | 58
                | 35
                | 64
                | 33
                | 38
                | 39
                | 44
                | 59
                | 61
                | 47
                | 42
                | 46
                | 63
                | 43
                | 36
                | 91
                | 93
                | 40
                | 41
        )
}

fn digit_width(text: &[u16], facts: &[u8], index: usize) -> usize {
    if facts.get(index).is_none_or(|fact| fact & 16 == 0) {
        return 0;
    }
    if (0xd800..=0xdbff).contains(&text[index])
        && text
            .get(index + 1)
            .is_some_and(|unit| (0xdc00..=0xdfff).contains(unit))
        && facts[index + 1] & 16 != 0
    {
        2
    } else {
        1
    }
}

fn link_end(text: &[u16], facts: &[u8], start: usize) -> Option<usize> {
    let rest = &text[start..];
    let prefix = if rest.starts_with(&[104, 116, 116, 112, 58, 47, 47]) {
        7
    } else if rest.starts_with(&[104, 116, 116, 112, 115, 58, 47, 47]) {
        8
    } else {
        return None;
    };
    let mut end = start + prefix;
    if !text.get(end).is_some_and(|unit| host(*unit)) {
        return None;
    }
    while text.get(end).is_some_and(|unit| host(*unit)) {
        end += 1;
    }
    let mut segments = 0;
    while text.get(end) == Some(&46) && text.get(end + 1).is_some_and(|unit| host(*unit)) {
        segments += 1;
        end += 1;
        while text.get(end).is_some_and(|unit| host(*unit)) {
            end += 1;
        }
    }
    if segments == 0 {
        return None;
    }
    if text.get(end) == Some(&58) {
        let mut port = end + 1;
        for _ in 0..5 {
            let width = digit_width(text, facts, port);
            if width == 0 {
                break;
            }
            port += width;
        }
        if port > end + 1 {
            end = port;
        }
    }
    if text.get(end) == Some(&47) {
        end += 1;
        while text.get(end).is_some_and(|unit| path(*unit)) {
            end += 1;
        }
    }
    Some(end)
}

/// Return non-overlapping UTF-16 start/end pairs matching the original fixed regex.
/// Existing `URLSpan` overlap remains an Android observation at application time.
///
/// # Errors
/// Rejects oversized text or malformed digit facts before producing any ranges.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "positions are bounded by MAX_TEXT, far below i32::MAX"
)]
pub fn links(text: &[u16], digits: &[u8]) -> Result<Vec<i32>, DisplayError> {
    validate(text, digits)?;
    let mut result = Vec::new();
    let mut index = 0;
    while index < text.len() {
        if let Some(end) = link_end(text, digits, index) {
            result.extend([index as i32, end as i32]);
            index = end;
        } else {
            index += 1;
        }
    }
    Ok(result)
}

fn int_value(text: &[u16], facts: &[u8]) -> Option<i32> {
    let negative = text.first() == Some(&45);
    let start = usize::from(negative || text.first() == Some(&43));
    if start == text.len() {
        return None;
    }
    let limit = if negative {
        2_147_483_648_u32
    } else {
        2_147_483_647
    };
    let mut value = 0_u32;
    for digit in &facts[start..] {
        let digit = digit & 15;
        if digit > 9 {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(digit))?;
        if value > limit {
            return None;
        }
    }
    let signed = i64::from(value) * if negative { -1 } else { 1 };
    i32::try_from(signed).ok()
}

/// Parse image URL dimensions; FIT scaling remains with the existing Coil renderer.
/// Returns source width/height and target width/height, or none for the 200x300 default.
///
/// # Errors
/// Rejects invalid observations and source-number overflow, preserving `NumberFormatException`.
#[allow(
    clippy::missing_panics_doc,
    reason = "the fixed compiled grammar always supplies its three declared captures"
)]
pub fn image_dimensions(text: &[u16], digits: &[u8]) -> Result<Option<[i32; 4]>, DisplayError> {
    static GRAMMAR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:[^-]+-){2}([0-9]+)-([0-9]+)-[^_]+_([^.]+)").expect("fixed image grammar")
    });
    validate(text, digits)?;
    // One ASCII byte per original UTF-16 unit preserves capture offsets. Only
    // the grammar's punctuation and platform-observed digit class matter here.
    let mapped: String = text
        .iter()
        .zip(digits)
        .map(|(unit, digit)| match *unit {
            45 => '-',
            95 => '_',
            46 => '.',
            _ if digit & 16 != 0 => '0',
            _ => 'x',
        })
        .collect();
    let Some(captures) = GRAMMAR.captures(&mapped) else {
        return Ok(None);
    };
    let value = |index| {
        let group = captures.get(index).expect("fixed capture");
        int_value(&text[group.range()], &digits[group.range()])
    };
    let width = value(1).ok_or(DisplayError::Number)?;
    let height = value(2).ok_or(DisplayError::Number)?;
    let target = value(3).unwrap_or(200);
    Ok(Some([width, height, target, (target / 2).wrapping_mul(3)]))
}

#[cfg(test)]
mod tests;
