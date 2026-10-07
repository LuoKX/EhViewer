//! Source-named std-only JVM decoder parity, bounds, and redaction tests.
#![deny(missing_docs, unsafe_code, clippy::all, clippy::pedantic)]

#[path = "../src/jvm_utf8.rs"]
mod jvm_utf8;

use jvm_utf8::{Error, MAX_INPUT_BYTES, MAX_OUTPUT_BYTES, decode};
use std::fs::File;
use std::io::Read as _;

const GOLDEN_SIZE: usize = 5_174_663;
const GOLDEN_CASES: usize = 258_862;

fn check(condition: bool) {
    assert!(condition, "synthetic decoder condition failed");
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], ()> {
        let result = self.0.get(..length).ok_or(())?;
        self.0 = self.0.get(length..).ok_or(())?;
        Ok(result)
    }
    fn byte(&mut self) -> Result<usize, ()> {
        Ok(usize::from(*self.take(1)?.first().ok_or(())?))
    }
    fn integer(&mut self, length: usize) -> Result<usize, ()> {
        Ok(self
            .take(length)?
            .iter()
            .fold(0, |number, byte| (number << 8) | usize::from(*byte)))
    }
}
fn replay(bytes: &[u8], expected_cases: usize) -> Result<(), ()> {
    if bytes.len() > GOLDEN_SIZE {
        return Err(());
    }
    let mut cursor = Cursor(bytes);
    if cursor.take(4)? != b"JU01" {
        return Err(());
    }
    let mut count = 0;
    loop {
        let input_length = cursor.byte()?;
        if input_length == 255 {
            if cursor.integer(4)? != count || count != expected_cases || !cursor.0.is_empty() {
                return Err(());
            }
            return Ok(());
        }
        if input_length > 64 || count >= expected_cases {
            return Err(());
        }
        let input = cursor.take(input_length)?;
        let units = cursor.byte()?;
        if units > 64 {
            return Err(());
        }
        let utf16 = cursor.take(units * 2)?;
        let length = cursor.integer(2)?;
        if length > 192 {
            return Err(());
        }
        let canonical = cursor.take(length)?;
        let actual = decode(input, MAX_OUTPUT_BYTES).map_err(|_| ())?;
        if actual.as_str().as_bytes() != canonical
            || !actual
                .as_str()
                .encode_utf16()
                .flat_map(u16::to_be_bytes)
                .eq(utf16.iter().copied())
        {
            return Err(());
        }
        count += 1;
    }
}

#[test]
fn jvm_utf8_real_generated_map_value_set_oracle_replay() {
    let root = option_env!("CARGO_MANIFEST_DIR").map_or_else(
        || std::path::PathBuf::from("native/parity/jvm-utf8/build"),
        |manifest| std::path::Path::new(manifest).join("parity/jvm-utf8/build"),
    );
    let mut bytes = Vec::new();
    let file = File::open(std::env::var_os("JVM_UTF8_BUILD").map_or_else(
        || root.join("oracle.bin"),
        |build| std::path::PathBuf::from(build).join("oracle.bin"),
    ))
    .expect("synthetic oracle unavailable");
    file.take(u64::try_from(GOLDEN_SIZE + 1).expect("fixture ceiling"))
        .read_to_end(&mut bytes)
        .expect("synthetic oracle read failed");
    assert!(bytes.len() == GOLDEN_SIZE, "synthetic oracle size mismatch");
    assert!(
        replay(&bytes, GOLDEN_CASES).is_ok(),
        "synthetic JVM oracle mismatch or malformed"
    );
}

#[test]
fn jvm_utf8_consumption_surrogates_not_rust_lossy_and_scalar_extremes() {
    let input = [0xed, 0xa0, 0x80, b'x', 0xf0, 0x90, 0x80];
    let actual = decode(&input, 7).expect("bounded decode");
    check(actual.as_str() == "\u{fffd}x\u{fffd}");
    check(actual.as_str() != String::from_utf8_lossy(&input));
    check(decode(&[0xed, 0xa0, b'x'], 4).is_ok_and(|text| text.as_str() == "\u{fffd}x"));
    let extremes = "\0\u{7f}\u{80}\u{7ff}\u{800}\u{d7ff}\u{e000}\u{ffff}\u{10000}\u{10ffff}";
    check(decode(extremes.as_bytes(), extremes.len()).is_ok_and(|text| text.as_str() == extremes));
}

#[test]
fn jvm_utf8_byte_budget_edges_no_partial_or_dropped_bytes() {
    check(decode(&[], 0).is_ok_and(|text| text.as_str().is_empty()));
    check(decode(&[255], 2) == Err(Error::LimitExceeded));
    check(decode(&[255], 3).is_ok_and(|text| text.into_string() == "\u{fffd}"));
    check(decode(&[b'x', 255], 3) == Err(Error::LimitExceeded));
    check(decode(&[], MAX_OUTPUT_BYTES + 1) == Err(Error::LimitExceeded));
    check(decode(&vec![0; MAX_INPUT_BYTES + 1], 0) == Err(Error::LimitExceeded));
    let ascii = vec![b'x'; MAX_INPUT_BYTES];
    check(
        decode(&ascii, MAX_OUTPUT_BYTES).is_ok_and(|text| text.as_str().len() == MAX_INPUT_BYTES),
    );
    check(decode(&ascii, MAX_OUTPUT_BYTES - 1) == Err(Error::LimitExceeded));
    let replacements = vec![255; MAX_OUTPUT_BYTES / 3];
    check(
        decode(&replacements, MAX_OUTPUT_BYTES)
            .is_ok_and(|text| text.as_str().len() == replacements.len() * 3),
    );
    check(
        decode(&vec![255; MAX_OUTPUT_BYTES / 3 + 1], MAX_OUTPUT_BYTES) == Err(Error::LimitExceeded),
    );
}

#[test]
fn jvm_utf8_debug_display_errors_redact_input() {
    let secret = decode(b"synthetic-private-marker", 64).expect("bounded decode");
    check(format!("{secret:?}") == "Decoded([redacted])");
    check(secret.clone() == secret);
    for (error, message) in [
        (Error::LimitExceeded, "JVM UTF8 limit exceeded"),
        (Error::AllocationFailed, "JVM UTF8 allocation failed"),
    ] {
        check(error.to_string() == message);
        let boxed: Box<dyn std::error::Error> = Box::new(error);
        check(boxed.source().is_none());
        check(!format!("{error:?}").contains("synthetic-private-marker"));
    }
}

#[test]
fn jvm_utf8_malformed_golden_shapes_fail_closed() {
    let empty = b"JU01\xff\0\0\0\0";
    check(replay(empty, 0).is_ok());
    for index in 0..empty.len() {
        check(replay(&empty[..index], 0).is_err());
    }
    for bytes in [
        b"BAD!\xff\0\0\0\0".as_slice(),
        b"JU01\xff\0\0\0\x01",
        b"JU01\xff\0\0\0\0x",
        b"JU01\x41",
        b"JU01\0\x41",
        b"JU01\0\0\0\xc1",
        b"JU01\x01x\0\0\0\xff\0\0\0\x01",
    ] {
        check(replay(bytes, 1).is_err());
    }
    check(replay(&vec![0; GOLDEN_SIZE + 1], 0).is_err());
}
