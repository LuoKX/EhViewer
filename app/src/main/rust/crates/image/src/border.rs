//! Detection of uniform black or white borders around reader images.

use anyhow::{Context as _, Result, ensure};
use image::{ImageBuffer, Luma};

use crate::pixel::{CustomPixel, ImageConsumer};

const FILLED_RATIO_DENOMINATOR: usize = 800;
const FILLED_RATIO_ROUNDING: usize = FILLED_RATIO_DENOMINATOR / 2;
// These retain the legacy 75% threshold after its float-to-u8 conversion.
const BLACK_THRESHOLD: u8 = 191;
const WHITE_THRESHOLD: u8 = 63;

fn is_white(pixel: Luma<u8>) -> bool {
    pixel.0[0] > WHITE_THRESHOLD
}

fn is_black(pixel: Luma<u8>) -> bool {
    pixel.0[0] < BLACK_THRESHOLD
}

fn line_contains_content<'pixel, P: CustomPixel + 'pixel>(
    line: impl Iterator<Item = &'pixel P>,
    white_border: bool,
    filled_limit: usize,
) -> bool {
    let differs_from_border = if white_border { is_black } else { is_white };
    line.step_by(2)
        .filter(|pixel| differs_from_border(pixel.to_luma8()))
        .count()
        > filled_limit
}

fn count_border_lines<'pixel, P: CustomPixel + 'pixel>(
    iter: impl Iterator<Item = impl Iterator<Item = &'pixel P>>,
    white_border: bool,
    filled_limit: usize,
) -> u32 {
    let mut count = 1_u32;
    for line in iter {
        if line_contains_content(line, white_border, filled_limit) {
            break;
        }
        count = count.saturating_add(1);
    }
    count
}

fn detect_border_lines<'pixel, P: CustomPixel + 'pixel>(
    mut lines: impl Iterator<Item = impl Iterator<Item = &'pixel P>>,
    line_length: u32,
) -> u32 {
    let Some(first_line) = lines.next() else {
        return 0;
    };
    let filled_limit = usize::try_from(line_length).map_or(usize::MAX, |length| {
        length.saturating_add(FILLED_RATIO_ROUNDING) / FILLED_RATIO_DENOMINATOR
    });
    let (mut black, mut white) = (0_usize, 0_usize);
    for pixel in first_line.step_by(2) {
        let luminance = pixel.to_luma8();
        if is_black(luminance) {
            black += 1;
        } else if is_white(luminance) {
            white += 1;
        }
    }
    match (black > filled_limit, white > filled_limit) {
        (true, false) => count_border_lines(lines, false, filled_limit),
        (false, true) => count_border_lines(lines, true, filled_limit),
        _ => 0,
    }
}

/// Detects the content rectangle after uniform black or white borders are removed.
///
/// The result is `[left, top, right, bottom]` in pixels. Android keeps the full
/// image when the returned rectangle is too small, so ambiguous edges return the
/// uncropped coordinate on that side.
#[derive(Debug, Clone, Copy, Default)]
pub struct DetectBorder;

impl ImageConsumer<[i32; 4]> for DetectBorder {
    fn apply<P: CustomPixel>(self, image: &ImageBuffer<P, &[P::Subpixel]>) -> Result<[i32; 4]> {
        let (width, height) = image.dimensions();
        ensure!(width > 0 && height > 0, "image dimensions must be non-zero");

        let top = detect_border_lines(image.rows(), width);
        let bottom = detect_border_lines(image.rows().rev(), width);
        let columns =
            (0..width).map(|column| (0..height).map(move |row| image.get_pixel(column, row)));
        let left = detect_border_lines(columns.clone(), height);
        let right = detect_border_lines(columns.rev(), height);

        let width = i32::try_from(width).context("image width exceeds the JNI coordinate range")?;
        let height =
            i32::try_from(height).context("image height exceeds the JNI coordinate range")?;
        let left = i32::try_from(left).context("left border exceeds the JNI coordinate range")?;
        let top = i32::try_from(top).context("top border exceeds the JNI coordinate range")?;
        let right =
            i32::try_from(right).context("right border exceeds the JNI coordinate range")?;
        let bottom =
            i32::try_from(bottom).context("bottom border exceeds the JNI coordinate range")?;
        Ok([left, top, width - right, height - bottom])
    }
}

#[cfg(test)]
mod tests {
    use half::f16;
    use image::{ImageBuffer, Rgba};

    use super::DetectBorder;
    use crate::pixel::{ImageConsumer, Rgb565, Rgba8888, RgbaF16};

    #[test]
    fn detects_black_and_white_uniform_borders() {
        let black = bordered_image(Rgba([0, 0, 0, 255]), Rgba([255, 255, 255, 255]));
        assert_eq!(
            DetectBorder.apply(&borrow(&black)).expect("black border"),
            [2, 1, 10, 9]
        );

        let white = bordered_image(Rgba([255, 255, 255, 255]), Rgba([0, 0, 0, 255]));
        assert_eq!(
            DetectBorder.apply(&borrow(&white)).expect("white border"),
            [2, 1, 10, 9]
        );
    }

    #[test]
    fn keeps_ambiguous_edges_and_rejects_empty_images() {
        let mixed_owned = ImageBuffer::from_fn(4, 4, |column, row| {
            let white = ((row == 0 || row == 3) && column == 2)
                || ((column == 0 || column == 3) && row == 2);
            if white {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let mixed = borrow(&mixed_owned);
        assert_eq!(
            DetectBorder.apply(&mixed).expect("mixed edges"),
            [0, 0, 4, 4]
        );

        let empty = ImageBuffer::<Rgba8888, _>::from_raw(0, 0, [].as_slice())
            .expect("an empty image has no pixel storage");
        assert!(
            DetectBorder
                .apply(&empty)
                .expect_err("empty image must be rejected")
                .to_string()
                .contains("non-zero")
        );
    }

    #[test]
    fn detects_the_same_border_for_packed_android_pixel_formats() {
        let rgb565 = packed_image(0_u16, u16::MAX);
        let borrowed_rgb565 = ImageBuffer::<Rgb565, _>::from_raw(5, 5, rgb565.as_slice())
            .expect("the RGB565 fixture matches its dimensions");
        assert_eq!(
            DetectBorder.apply(&borrowed_rgb565).expect("RGB565 border"),
            [1, 1, 4, 4]
        );

        let alpha = u64::from(f16::from_f32(1.0).to_bits()) << 48;
        let white_channel = u64::from(f16::from_f32(1.0).to_bits());
        let white = white_channel | white_channel << 16 | white_channel << 32 | alpha;
        let rgba_f16 = packed_image(alpha, white);
        let borrowed_rgba_f16 = ImageBuffer::<RgbaF16, _>::from_raw(5, 5, rgba_f16.as_slice())
            .expect("the RGBAF16 fixture matches its dimensions");
        assert_eq!(
            DetectBorder
                .apply(&borrowed_rgba_f16)
                .expect("RGBAF16 border"),
            [1, 1, 4, 4]
        );
    }

    fn bordered_image(border: Rgba8888, content: Rgba8888) -> ImageBuffer<Rgba8888, Vec<u8>> {
        ImageBuffer::from_fn(12, 10, |column, row| {
            if (2..10).contains(&column) && (1..9).contains(&row) {
                content
            } else {
                border
            }
        })
    }

    fn borrow(image: &ImageBuffer<Rgba8888, Vec<u8>>) -> ImageBuffer<Rgba8888, &[u8]> {
        ImageBuffer::from_raw(image.width(), image.height(), image.as_raw().as_slice())
            .expect("the owned image dimensions match its storage")
    }

    fn packed_image<T: Copy>(border: T, content: T) -> Vec<T> {
        (0..25)
            .map(|index| {
                let column = index % 5;
                let row = index / 5;
                if (1..4).contains(&column) && (1..4).contains(&row) {
                    content
                } else {
                    border
                }
            })
            .collect()
    }
}
