//! Download page naming, fixed URL hash grammar and post-archive selection.
//! Android supplies locale zero-digit and ICU filename-match observations.

/// Maximum extension code units.
pub const MAX_EXTENSION: usize = 4096;
/// Maximum URL code units.
pub const MAX_URL: usize = 16 * 1024;
/// Maximum observed cleanup entries.
pub const MAX_FILES: usize = 100_000;

/// Formats the wrapped one-based page with the platform locale's decimal digits.
///
/// # Errors
/// Rejects oversized extensions or an impossible contiguous decimal digit range.
pub fn page_filename(
    index: i32,
    zero_digit: u16,
    extension: &[u16],
) -> Result<Vec<u16>, &'static str> {
    if extension.len() > MAX_EXTENSION || zero_digit > u16::MAX - 9 {
        return Err("Invalid download filename input");
    }
    let number = index.wrapping_add(1);
    let digits = number.unsigned_abs().to_string();
    let negative = usize::from(number < 0);
    let mut name = Vec::with_capacity(extension.len() + 12);
    if number < 0 {
        name.push(u16::from(b'-'));
    }
    name.extend(std::iter::repeat_n(
        zero_digit,
        8_usize.saturating_sub(digits.len() + negative),
    ));
    name.extend(
        digits
            .bytes()
            .map(|digit| zero_digit + u16::from(digit - b'0')),
    );
    name.push(u16::from(b'.'));
    name.extend_from_slice(extension);
    Ok(name)
}

/// Finds the first `/h/` followed by forty lowercase hexadecimal code units.
/// Extra following characters are deliberately not an end-of-match constraint.
///
/// # Errors
/// Rejects oversized URLs before searching. URLs never appear in errors.
pub fn expected_image_hash(url: &[u16]) -> Result<Option<String>, &'static str> {
    if url.len() > MAX_URL {
        return Err("Download URL exceeds limit");
    }
    let found = url.windows(43).find(|window| {
        window[..3] == [47, 104, 47]
            && window[3..]
                .iter()
                .all(|unit| (48..=57).contains(unit) || (97..=102).contains(unit))
    });
    Ok(found.map(|window| {
        window[3..]
            .iter()
            .map(|unit| char::from(unit.to_le_bytes()[0]))
            .collect()
    }))
}

/// Selects source-aligned file slots from Android ICU observations in one batch.
/// Filesystem authorization, lifecycle and deletion stay with Android.
///
/// # Errors
/// Rejects an oversized snapshot before returning any deletion intent.
pub fn post_archive_pages(matches: &[bool]) -> Result<Vec<i32>, &'static str> {
    if matches.len() > MAX_FILES {
        return Err("Download cleanup exceeds limit");
    }
    Ok(matches
        .iter()
        .zip(0_i32..)
        .filter(|(matched, _)| **matched)
        .map(|(_, index)| index)
        .collect())
}

#[cfg(test)]
mod tests;
