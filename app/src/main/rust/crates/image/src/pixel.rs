//! Android pixel-format normalization for host-testable algorithms.

use anyhow::Result;
use half::f16;
use image::{ImageBuffer, Luma, Pixel, Rgb, Rgba};

/// Android `RGBA_8888` pixels represented as four eight-bit channels.
pub type Rgba8888 = Rgba<u8>;

/// Android `RGB_565` pixels represented as one packed sixteen-bit value.
pub type Rgb565 = Luma<u16>;

/// Android `RGBA_F16` pixels represented as one packed sixty-four-bit value.
pub type RgbaF16 = Luma<u64>;

/// A computation that consumes an immutable borrowed image without owning its storage.
pub trait ImageConsumer<R> {
    /// Applies the computation to `buffer` and returns its derived result.
    ///
    /// Implementations must not retain the borrowed pixel storage after this call.
    ///
    /// # Errors
    ///
    /// Returns an implementation-specific error when the supplied dimensions or
    /// image contents cannot produce a valid result.
    fn apply<T: CustomPixel>(self, buffer: &ImageBuffer<T, &[T::Subpixel]>) -> Result<R>;
}

/// A supported Android pixel whose luminance can be normalized to eight bits.
pub trait CustomPixel: Pixel {
    /// Converts this pixel to an eight-bit luminance value used by image analysis.
    fn to_luma8(&self) -> Luma<u8>;
}

impl CustomPixel for Rgba8888 {
    fn to_luma8(&self) -> Luma<u8> {
        self.to_luma()
    }
}

impl CustomPixel for Rgb565 {
    fn to_luma8(&self) -> Luma<u8> {
        let packed = self.channels()[0];
        let rgb = Rgb([
            five_bits_to_eight(packed >> 11),
            six_bits_to_eight(packed >> 5),
            five_bits_to_eight(packed),
        ]);
        rgb.to_luma()
    }
}

#[inline]
fn five_bits_to_eight(value: u16) -> u8 {
    u8::try_from(((value & 0x1f) * 527 + 23) >> 6).unwrap_or(u8::MAX)
}

#[inline]
fn six_bits_to_eight(value: u16) -> u8 {
    u8::try_from(((value & 0x3f) * 259 + 33) >> 6).unwrap_or(u8::MAX)
}

impl CustomPixel for RgbaF16 {
    fn to_luma8(&self) -> Luma<u8> {
        let packed = self.channels()[0];
        let rgba = Rgba([
            half_to_f32(packed),
            half_to_f32(packed >> 16),
            half_to_f32(packed >> 32),
            half_to_f32(packed >> 48),
        ]);
        let luminance = rgba.to_luma().channels()[0];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the value is clamped to the documented u8 range before conversion"
        )]
        let normalized = (luminance.clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8;
        Luma([normalized])
    }
}

#[inline]
fn half_to_f32(value: u64) -> f32 {
    f16::from_bits(u16::try_from(value & 0xffff).unwrap_or_default()).to_f32()
}

#[cfg(test)]
mod tests {
    use half::f16;
    use image::{Luma, Pixel, Rgb, Rgba};

    use super::{CustomPixel, Rgb565, RgbaF16};

    #[test]
    fn normalizes_rgba8888_and_rgb565_luminance() {
        assert_eq!(Rgba([0_u8, 0, 0, 255]).to_luma8(), Luma([0]));
        assert_eq!(Rgba([255_u8, 255, 255, 255]).to_luma8(), Luma([255]));

        let red: Rgb565 = Luma([0b1111_1000_0000_0000]);
        assert_eq!(red.to_luma8(), Rgb([255_u8, 0, 0]).to_luma());
        let white: Rgb565 = Luma([u16::MAX]);
        assert_eq!(white.to_luma8(), Luma([255]));
    }

    #[test]
    fn normalizes_and_clamps_packed_rgba_f16_luminance() {
        let white_bits = u64::from(f16::from_f32(1.0).to_bits());
        let white: RgbaF16 =
            Luma([white_bits | white_bits << 16 | white_bits << 32 | white_bits << 48]);
        assert_eq!(white.to_luma8(), Luma([255]));

        let high_bits = u64::from(f16::from_f32(2.0).to_bits());
        let high: RgbaF16 =
            Luma([high_bits | high_bits << 16 | high_bits << 32 | white_bits << 48]);
        assert_eq!(high.to_luma8(), Luma([255]));

        let low_bits = u64::from(f16::from_f32(-1.0).to_bits());
        let low: RgbaF16 = Luma([low_bits | low_bits << 16 | low_bits << 32 | white_bits << 48]);
        assert_eq!(low.to_luma8(), Luma([0]));

        let red: RgbaF16 = Luma([white_bits | white_bits << 48]);
        let expected = Rgba([1.0_f32, 0.0, 0.0, 1.0]).to_luma().channels()[0];
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the expected test value is clamped before conversion"
        )]
        let expected = (expected.clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8;
        assert_eq!(red.to_luma8(), Luma([expected]));
    }
}
