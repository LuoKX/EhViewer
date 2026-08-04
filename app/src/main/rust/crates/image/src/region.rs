//! Safe byte-row copying for cropped image regions.
//!
//! Android bitmap and hardware-buffer adapters validate their native handles,
//! expose bounded byte slices, and delegate the actual row/stride arithmetic to
//! this module. This keeps pointer ownership outside the host-testable crate.

use anyhow::{Context, Result, ensure};

/// Describes one packed-pixel image stored in byte-addressed rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageLayout {
    width: usize,
    height: usize,
    row_stride_bytes: usize,
    row_bytes: usize,
    required_len: usize,
    bytes_per_pixel: usize,
}

impl ImageLayout {
    /// Validates and creates an image layout.
    ///
    /// `row_stride_bytes` may exceed the logical row size when native storage
    /// contains padding. `bytes_per_pixel` is the complete packed-pixel size.
    ///
    /// # Errors
    ///
    /// Returns an error for zero dimensions/pixel size, a stride shorter than a
    /// logical row, a platform-size conversion failure, or address arithmetic
    /// overflow.
    pub fn new(
        width: u32,
        height: u32,
        row_stride_bytes: usize,
        bytes_per_pixel: usize,
    ) -> Result<Self> {
        ensure!(width > 0 && height > 0, "image dimensions must be non-zero");
        ensure!(bytes_per_pixel > 0, "image pixel size must be non-zero");
        let width = usize::try_from(width).context("image width exceeds this platform")?;
        let height = usize::try_from(height).context("image height exceeds this platform")?;
        let row_bytes = width
            .checked_mul(bytes_per_pixel)
            .context("image row size overflow")?;
        ensure!(
            row_stride_bytes >= row_bytes,
            "image row stride is shorter than its logical width"
        );
        let required_len = height
            .saturating_sub(1)
            .checked_mul(row_stride_bytes)
            .and_then(|offset| offset.checked_add(row_bytes))
            .context("image storage size overflow")?;
        Ok(Self {
            width,
            height,
            row_stride_bytes,
            row_bytes,
            required_len,
            bytes_per_pixel,
        })
    }

    /// Returns the minimum byte-slice length required by this layout.
    #[must_use]
    pub const fn required_len(self) -> usize {
        self.required_len
    }
}

/// Identifies a non-empty rectangular source region in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Region {
    /// Creates a source region from pixel coordinates and dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error for a zero width/height or a platform-size conversion
    /// failure. Image-bound validation occurs in [`copy_to_origin`].
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self> {
        ensure!(width > 0 && height > 0, "copy region must be non-empty");
        Ok(Self {
            x: usize::try_from(x).context("copy x-coordinate exceeds this platform")?,
            y: usize::try_from(y).context("copy y-coordinate exceeds this platform")?,
            width: usize::try_from(width).context("copy width exceeds this platform")?,
            height: usize::try_from(height).context("copy height exceeds this platform")?,
        })
    }
}

/// Copies `region` from `source` to the origin of `destination` row by row.
///
/// Source and destination padding remains untouched. Both layouts must use the
/// same packed-pixel size. The caller must provide disjoint slices; Rust's
/// borrowing rules enforce that contract for safe callers.
///
/// # Errors
///
/// Returns an error when a slice is shorter than its layout, pixel sizes differ,
/// the source region is out of bounds, the destination is too small, or checked
/// byte-offset arithmetic overflows.
pub fn copy_to_origin(
    source: &[u8],
    source_layout: ImageLayout,
    destination: &mut [u8],
    destination_layout: ImageLayout,
    region: Region,
) -> Result<()> {
    ensure!(
        source.len() >= source_layout.required_len,
        "source storage is shorter than its layout"
    );
    ensure!(
        destination.len() >= destination_layout.required_len,
        "destination storage is shorter than its layout"
    );
    ensure!(
        source_layout.bytes_per_pixel == destination_layout.bytes_per_pixel,
        "source and destination pixel sizes differ"
    );
    let source_right = region
        .x
        .checked_add(region.width)
        .context("source region horizontal bounds overflow")?;
    let source_bottom = region
        .y
        .checked_add(region.height)
        .context("source region vertical bounds overflow")?;
    ensure!(
        source_right <= source_layout.width && source_bottom <= source_layout.height,
        "source region exceeds image bounds"
    );
    ensure!(
        region.width <= destination_layout.width && region.height <= destination_layout.height,
        "destination image is smaller than the copy region"
    );

    let copy_bytes = region
        .width
        .checked_mul(source_layout.bytes_per_pixel)
        .context("copy row size overflow")?;
    let source_x_bytes = region
        .x
        .checked_mul(source_layout.bytes_per_pixel)
        .context("source x-offset overflow")?;
    for row in 0..region.height {
        let source_row = region
            .y
            .checked_add(row)
            .and_then(|index| index.checked_mul(source_layout.row_stride_bytes))
            .and_then(|offset| offset.checked_add(source_x_bytes))
            .context("source row offset overflow")?;
        let destination_row = row
            .checked_mul(destination_layout.row_stride_bytes)
            .context("destination row offset overflow")?;
        let source_end = source_row
            .checked_add(copy_bytes)
            .context("source row end overflow")?;
        let destination_end = destination_row
            .checked_add(copy_bytes)
            .context("destination row end overflow")?;
        destination[destination_row..destination_end]
            .copy_from_slice(&source[source_row..source_end]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ImageLayout, Region, copy_to_origin};

    #[test]
    fn copies_a_crop_across_padded_rows_without_touching_padding() {
        let source = [1, 2, 3, 90, 91, 4, 5, 6, 92, 93, 7, 8, 9];
        let source_layout = ImageLayout::new(3, 3, 5, 1).expect("source layout should be valid");
        let destination_layout =
            ImageLayout::new(2, 2, 4, 1).expect("destination layout should be valid");
        assert_eq!(source_layout.required_len(), 13);
        assert_eq!(destination_layout.required_len(), 6);
        let mut destination = [0_u8; 6];

        copy_to_origin(
            &source,
            source_layout,
            &mut destination,
            destination_layout,
            Region::new(1, 1, 2, 2).expect("region should be valid"),
        )
        .expect("in-bounds crop should copy");

        assert_eq!(destination, [5, 6, 0, 0, 8, 9]);
    }

    #[test]
    fn copies_multibyte_pixels_without_reencoding() {
        let source = [0, 1, 2, 3, 4, 5];
        let layout = ImageLayout::new(3, 1, 6, 2).expect("packed RGB565-like row is valid");
        let destination_layout =
            ImageLayout::new(2, 1, 4, 2).expect("destination row should be valid");
        let mut destination = [0_u8; 4];

        copy_to_origin(
            &source,
            layout,
            &mut destination,
            destination_layout,
            Region::new(1, 0, 2, 1).expect("region should be valid"),
        )
        .expect("packed pixels should copy byte-for-byte");

        assert_eq!(destination, [2, 3, 4, 5]);
    }

    #[test]
    fn rejects_invalid_layouts_regions_and_storage() {
        assert!(ImageLayout::new(0, 1, 1, 1).is_err());
        assert!(ImageLayout::new(1, 0, 1, 1).is_err());
        assert!(ImageLayout::new(1, 1, 1, 0).is_err());
        assert!(ImageLayout::new(2, 1, 1, 1).is_err());
        assert!(ImageLayout::new(1, u32::MAX, usize::MAX, 1).is_err());
        assert!(Region::new(0, 0, 0, 1).is_err());
        assert!(Region::new(0, 0, 1, 0).is_err());

        let source_layout = ImageLayout::new(2, 2, 2, 1).expect("source layout should be valid");
        let destination_layout =
            ImageLayout::new(1, 1, 1, 1).expect("destination layout should be valid");
        let source = [1_u8, 2, 3, 4];
        let mut destination = [0_u8; 1];
        assert!(
            copy_to_origin(
                &source[..3],
                source_layout,
                &mut destination,
                destination_layout,
                Region::new(0, 0, 1, 1).expect("region should be valid"),
            )
            .is_err()
        );
        assert!(
            copy_to_origin(
                &source,
                source_layout,
                &mut [],
                destination_layout,
                Region::new(0, 0, 1, 1).expect("region should be valid"),
            )
            .is_err()
        );
        let mismatched = ImageLayout::new(1, 1, 2, 2).expect("layout should be valid");
        assert!(
            copy_to_origin(
                &source,
                source_layout,
                &mut [0_u8; 2],
                mismatched,
                Region::new(0, 0, 1, 1).expect("region should be valid"),
            )
            .is_err()
        );
        assert!(
            copy_to_origin(
                &source,
                source_layout,
                &mut destination,
                destination_layout,
                Region::new(2, 0, 1, 1).expect("region should be valid"),
            )
            .is_err()
        );
        assert!(
            copy_to_origin(
                &source,
                source_layout,
                &mut destination,
                destination_layout,
                Region::new(0, 2, 1, 1).expect("region should be valid"),
            )
            .is_err()
        );
        assert!(
            copy_to_origin(
                &source,
                source_layout,
                &mut destination,
                destination_layout,
                Region::new(0, 0, 2, 1).expect("region should be valid"),
            )
            .is_err()
        );
    }
}
