//! Thin hooks into application-owned resource policy.
//! no_std builds retain upstream behavior.

#[cfg(feature = "std")]
pub use eh_codec_limits::{DictionaryReservation, check_dictionary, check_index, reserve_dictionary, with_decoder_limits};

/// Preserve upstream no_std dictionary behavior.
#[cfg(not(feature = "std"))]
pub fn check_dictionary(_: usize) -> crate::Result<()> {
    Ok(())
}

/// Preserve upstream no_std XZ-index behavior.
#[cfg(not(feature = "std"))]
pub fn check_index(_: u64) -> crate::Result<()> {
    Ok(())
}
