//! GIF header recognition and the Android 9–13 short-frame-delay workaround.

const GIF87A_HEADER: &[u8; 6] = b"GIF87a";
const GIF89A_HEADER: &[u8; 6] = b"GIF89a";
const CONTROL_EXTENSION_PREFIX: &[u8; 4] = b"\0\x21\xf9\x04";
const CONTROL_EXTENSION_BYTES: usize = 9;
const MINIMUM_FRAME_DELAY: u16 = 2;
const REPLACEMENT_FRAME_DELAY: u16 = 10;

/// Returns whether `source` starts with an exact `GIF87a` or `GIF89a` header.
#[must_use]
pub fn is_gif(source: &[u8]) -> bool {
    source.starts_with(GIF87A_HEADER) || source.starts_with(GIF89A_HEADER)
}

/// Rewrites legacy zero- and one-centisecond frame delays to ten centiseconds.
///
/// This preserves the historical workaround's marker grammar: the Graphic
/// Control Extension must be preceded by a zero block terminator, have a zero
/// terminator of its own, and appear before the first well-formed extension
/// whose little-endian delay is at least two. Truncated or non-GIF input is a
/// safe no-op.
///
/// The returned count is the number of delay fields changed.
#[must_use]
pub fn rewrite_short_frame_delays(source: &mut [u8]) -> usize {
    if !is_gif(source) || source.len() < CONTROL_EXTENSION_BYTES {
        return 0;
    }

    let mut rewritten = 0;
    for index in 0..=source.len() - CONTROL_EXTENSION_BYTES {
        if &source[index..index + CONTROL_EXTENSION_PREFIX.len()] != CONTROL_EXTENSION_PREFIX {
            continue;
        }

        let extension = &mut source[index + CONTROL_EXTENSION_PREFIX.len()..];
        if extension[4] != 0 {
            continue;
        }
        let delay = u16::from_le_bytes([extension[1], extension[2]]);
        if delay >= MINIMUM_FRAME_DELAY {
            break;
        }
        [extension[1], extension[2]] = REPLACEMENT_FRAME_DELAY.to_le_bytes();
        rewritten += 1;
    }
    rewritten
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST_ZERO: &[u8] = b"\0\x21\xf9\x04\0\0\0\0\0";
    const FAST_ONE: &[u8] = b"\0\x21\xf9\x04\0\x01\0\0\0";
    const NORMAL_TWO: &[u8] = b"\0\x21\xf9\x04\0\x02\0\0\0";

    #[test]
    fn recognizes_only_complete_exact_gif_headers() {
        assert!(is_gif(b"GIF87a"));
        assert!(is_gif(b"GIF89a payload"));
        assert!(!is_gif(b"GIF87"));
        assert!(!is_gif(b"gif89a"));
        assert!(!is_gif(b"NOTGIF payload"));
    }

    #[test]
    fn rewrites_zero_and_one_delays_for_both_versions() {
        for header in [GIF87A_HEADER, GIF89A_HEADER] {
            let mut source = [header.as_slice(), FAST_ZERO, FAST_ONE].concat();
            assert_eq!(rewrite_short_frame_delays(&mut source), 2);
            assert_eq!(&source[11..13], &[10, 0]);
            assert_eq!(&source[20..22], &[10, 0]);
        }
    }

    #[test]
    fn first_normal_delay_stops_the_historical_scan() {
        let mut source = [GIF89A_HEADER.as_slice(), FAST_ONE, NORMAL_TWO, FAST_ZERO].concat();

        assert_eq!(rewrite_short_frame_delays(&mut source), 1);
        assert_eq!(&source[11..13], &[10, 0]);
        assert_eq!(&source[29..31], &[0, 0]);
    }

    #[test]
    fn ignores_wrong_boundaries_and_nonzero_extension_terminators() {
        let without_preceding_zero = b"\x7f\x21\xf9\x04\0\x01\0\0\0";
        let nonzero_terminator = b"\0\x21\xf9\x04\0\x01\0\0\x7f";
        let mut source = [
            GIF89A_HEADER.as_slice(),
            without_preceding_zero,
            nonzero_terminator,
            FAST_ONE,
        ]
        .concat();

        assert_eq!(rewrite_short_frame_delays(&mut source), 1);
        assert_eq!(&source[11..13], &[1, 0]);
        assert_eq!(&source[20..22], &[1, 0]);
        assert_eq!(&source[29..31], &[10, 0]);
    }

    #[test]
    fn every_truncated_length_and_non_gif_input_is_unchanged() {
        let full = [GIF89A_HEADER.as_slice(), FAST_ONE].concat();
        for length in 0..full.len() {
            let mut truncated = full[..length].to_vec();
            let original = truncated.clone();
            assert_eq!(rewrite_short_frame_delays(&mut truncated), 0);
            assert_eq!(truncated, original);
        }

        let mut non_gif = [b"NOTGIF".as_slice(), FAST_ONE].concat();
        let original = non_gif.clone();
        assert_eq!(rewrite_short_frame_delays(&mut non_gif), 0);
        assert_eq!(non_gif, original);
    }

    #[test]
    fn representative_large_source_is_rewritten_in_place() {
        let mut source = vec![0x55; 8 * 1024 * 1024];
        source[..GIF89A_HEADER.len()].copy_from_slice(GIF89A_HEADER);
        let marker = source.len() - FAST_ONE.len();
        source[marker..].copy_from_slice(FAST_ONE);
        let capacity = source.capacity();

        assert_eq!(rewrite_short_frame_delays(&mut source), 1);
        assert_eq!(source.capacity(), capacity);
        assert_eq!(&source[marker + 5..marker + 7], &[10, 0]);
    }
}
