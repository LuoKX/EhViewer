//! Reader border-crop eligibility and acceptance, preserving Kotlin arithmetic.
use anyhow::{Result, ensure};

/// Resolve one crop, invoking platform observations only when needed.
///
/// QR images bypass even dimension reads. Elongated images bypass pixel detection.
/// Threshold multiplication and rectangle subtraction match JVM Float/Int behavior.
///
/// # Errors
/// Propagates dimension/detection errors and rejects non-positive dimensions.
#[allow(
    clippy::cast_precision_loss,
    reason = "matches Kotlin Int-to-Float threshold arithmetic"
)]
pub fn plan_crop(
    has_qr_code: bool,
    dimensions: impl FnOnce() -> Result<(i32, i32)>,
    detect: impl FnOnce() -> Result<[i32; 4]>,
) -> Result<Option<[i32; 4]>> {
    if has_qr_code {
        return Ok(None);
    }
    let (width, height) = dimensions()?;
    ensure!(width > 0 && height > 0, "Invalid crop dimensions");
    let ratio = if height > width {
        height / width
    } else {
        width / height
    };
    if ratio >= 2 {
        return Ok(None);
    }
    let rect = detect()?;
    let accepted = rect[2].wrapping_sub(rect[0]) as f32 > width as f32 * 0.75
        && rect[3].wrapping_sub(rect[1]) as f32 > height as f32 * 0.75;
    Ok(accepted.then_some(rect))
}

#[cfg(test)]
mod tests;
