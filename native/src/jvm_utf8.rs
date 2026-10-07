//! Bounded `OpenJDK` UTF8 replacement decoding, frozen by synthetic JVM replay.
//!
//! Matches cached `DataStore` 1.2.1 generated value/set parsing on pinned host
//! JDK, not Android libcore/device execution. Map keys must remain strict UTF8.
//! Unlike Rust's lossy conversion, one complete UTF8-encoded surrogate consumes
//! three bytes and produces one replacement character. Incomplete or interrupted
//! sequences consume only their valid prefix; invalid range bytes are retried.

use std::fmt;

/// Hard input ceiling; caller must also enforce whole-document wire budgets.
pub const MAX_INPUT_BYTES: usize = 1_048_576;
/// Hard canonical UTF8 output ceiling, including replacement expansion.
pub const MAX_OUTPUT_BYTES: usize = 1_048_576;

/// Redacted failure categories; never include input, paths, or settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Input or requested/actual output exceeds its byte ceiling.
    LimitExceeded,
    /// Bounded output allocation failed.
    AllocationFailed,
}
impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LimitExceeded => "JVM UTF8 limit exceeded",
            Self::AllocationFailed => "JVM UTF8 allocation failed",
        })
    }
}
impl std::error::Error for Error {}

/// Canonical UTF8 with Java-equivalent UTF16 units; debug output is redacted.
#[derive(Clone, Eq, PartialEq)]
pub struct Decoded(String);
impl fmt::Debug for Decoded {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Decoded([redacted])")
    }
}
impl Decoded {
    /// Decoded text. Its byte length includes all replacement expansion.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Transfer canonical string without another allocation.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// Decode all bytes using pinned JVM replacement and byte-consumption rules.
///
/// `output_limit` is caller's remaining canonical UTF8 budget, not a UTF16-unit
/// count. Both input and output have one-MiB hard ceilings. Empty input accepts
/// zero output budget. Two passes validate expansion before allocation; neither
/// malformed input nor budget failure yields partial text. No bytes are dropped.
///
/// # Errors
/// Returns redacted `LimitExceeded` or `AllocationFailed`. Caller still owns
/// document-wide accounting, unknown-wire retention, and ordered-set dedup.
pub fn decode(input: &[u8], output_limit: usize) -> Result<Decoded, Error> {
    if input.len() > MAX_INPUT_BYTES || output_limit > MAX_OUTPUT_BYTES {
        return Err(Error::LimitExceeded);
    }
    let mut output_bytes = 0;
    let mut remaining = input;
    while !remaining.is_empty() {
        let (character, consumed) = next(remaining);
        output_bytes += character.len_utf8();
        if output_bytes > output_limit {
            return Err(Error::LimitExceeded);
        }
        remaining = &remaining[consumed..];
    }
    let mut output = String::new();
    if output.try_reserve_exact(output_bytes).is_err() {
        return Err(Error::AllocationFailed);
    }
    remaining = input;
    while !remaining.is_empty() {
        let (character, consumed) = next(remaining);
        output.push(character);
        remaining = &remaining[consumed..];
    }
    Ok(Decoded(output))
}

// Called only with nonempty bounded slices. Range checks match JVM malformed
// prefix consumption, including delayed surrogate rejection after byte three.
fn next(input: &[u8]) -> (char, usize) {
    let first = input[0];
    if first < 0x80 {
        return (char::from(first), 1);
    }
    let (width, mut scalar) = match first {
        0xc2..=0xdf => (2, u32::from(first & 0x1f)),
        0xe0..=0xef => (3, u32::from(first & 0x0f)),
        0xf0..=0xf4 => (4, u32::from(first & 0x07)),
        _ => return ('\u{fffd}', 1),
    };
    for offset in 1..width {
        let Some(&byte) = input.get(offset) else {
            return ('\u{fffd}', offset);
        };
        if !(0x80..=0xbf).contains(&byte)
            || (offset == 1
                && ((first == 0xe0 && byte < 0xa0)
                    || (first == 0xf0 && byte < 0x90)
                    || (first == 0xf4 && byte > 0x8f)))
        {
            return ('\u{fffd}', offset);
        }
        scalar = (scalar << 6) | u32::from(byte & 0x3f);
    }
    // Only nonscalar admitted by prefix checks is a complete encoded surrogate.
    (char::from_u32(scalar).unwrap_or('\u{fffd}'), width)
}
