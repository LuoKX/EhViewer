//! QR finder-pattern detection for reader safety checks.

use std::f32::consts::FRAC_1_SQRT_2;

use anyhow::Result;
use image::{GrayImage, ImageBuffer};
use rxing::common::HybridBinarizer;
use rxing::qrcode::detector::FinderPatternFinder;
use rxing::{BinaryBitmap, DecodeHints, Luma8LuminanceSource, Point};

use crate::pixel::{CustomPixel, ImageConsumer};

fn to_luminance_source<P: CustomPixel>(
    source: &ImageBuffer<P, &[P::Subpixel]>,
) -> Luma8LuminanceSource {
    let (width, height) = source.dimensions();
    let mut luminance = GrayImage::new(width, height);
    for (target, source) in luminance.pixels_mut().zip(source.pixels()) {
        *target = source.to_luma8();
    }
    Luma8LuminanceSource::new(luminance.into_raw(), width, height)
}

/// Detects a square arrangement of three QR finder patterns.
///
/// This deliberately does not decode the QR payload. Reader ad filtering only
/// needs a conservative signal that a likely QR code is present.
#[derive(Debug, Clone, Copy, Default)]
pub struct DetectQrCode;

impl ImageConsumer<bool> for DetectQrCode {
    fn apply<P: CustomPixel>(self, image: &ImageBuffer<P, &[P::Subpixel]>) -> Result<bool> {
        if image.width() == 0 || image.height() == 0 {
            return Ok(false);
        }
        let source = to_luminance_source(image);
        let bitmap = BinaryBitmap::new(HybridBinarizer::new(source));
        match FinderPatternFinder::new(bitmap.get_black_matrix()).find(&DecodeHints::default()) {
            Ok(info) => {
                let top_left = Point::from(info.getTopLeft());
                let top_right = Point::from(info.getTopRight());
                let bottom_left = Point::from(info.getBottomLeft());
                let side_length = top_right.distance(top_left);
                let tolerance = side_length * 0.05;
                Ok(
                    (bottom_left.distance(top_left) - side_length).abs() < tolerance
                        && (bottom_left.distance(top_right) * FRAC_1_SQRT_2 - side_length).abs()
                            < tolerance,
                )
            }
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    use super::DetectQrCode;
    use crate::pixel::{ImageConsumer, Rgba8888};

    #[test]
    fn detects_three_square_qr_finder_patterns() {
        let fixture = qr_finder_fixture();
        let image = borrow(&fixture);
        assert!(
            DetectQrCode
                .apply(&image)
                .expect("the synthetic QR fixture should be analyzable")
        );
    }

    #[test]
    fn rejects_blank_and_empty_images() {
        let blank = ImageBuffer::from_pixel(87, 87, Rgba([255_u8, 255, 255, 255]));
        assert!(
            !DetectQrCode
                .apply(&borrow(&blank))
                .expect("a blank image should be analyzable")
        );

        let empty = ImageBuffer::<Rgba8888, _>::from_raw(0, 0, [].as_slice())
            .expect("an empty image has no pixel storage");
        assert!(
            !DetectQrCode
                .apply(&empty)
                .expect("an empty image is explicitly not a QR code")
        );
    }

    fn qr_finder_fixture() -> ImageBuffer<Rgba8888, Vec<u8>> {
        let mut image = ImageBuffer::from_pixel(87, 87, Rgba([255, 255, 255, 255]));
        for (module_x, module_y) in [(4, 4), (18, 4), (4, 18)] {
            draw_finder_pattern(&mut image, module_x, module_y, 3);
        }
        image
    }

    fn draw_finder_pattern(
        image: &mut ImageBuffer<Rgba8888, Vec<u8>>,
        module_x: u32,
        module_y: u32,
        scale: u32,
    ) {
        for y in 0..7 {
            for x in 0..7 {
                let black = x == 0
                    || x == 6
                    || y == 0
                    || y == 6
                    || ((2..=4).contains(&x) && (2..=4).contains(&y));
                let color = if black {
                    Rgba([0, 0, 0, 255])
                } else {
                    Rgba([255, 255, 255, 255])
                };
                for pixel_y in 0..scale {
                    for pixel_x in 0..scale {
                        image.put_pixel(
                            (module_x + x) * scale + pixel_x,
                            (module_y + y) * scale + pixel_y,
                            color,
                        );
                    }
                }
            }
        }
    }

    fn borrow(image: &ImageBuffer<Rgba8888, Vec<u8>>) -> ImageBuffer<Rgba8888, &[u8]> {
        ImageBuffer::from_raw(image.width(), image.height(), image.as_raw().as_slice())
            .expect("the owned image dimensions match its storage")
    }
}
