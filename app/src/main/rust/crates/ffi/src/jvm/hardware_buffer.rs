//! API 26+ `HardwareBuffer` adapter for safe image-region copies.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::slice::{from_raw_parts, from_raw_parts_mut};

use anyhow::{Context, Result, ensure};
use eh_image::region::{ImageLayout, Region, copy_to_origin};
use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jint, jobject};
use jni_fn::jni_fn;
use ndk::bitmap::{Bitmap, BitmapFormat};
use ndk::hardware_buffer::{HardwareBuffer, HardwareBufferUsage};
use ndk::hardware_buffer_format::HardwareBufferFormat;

use super::bitmap::LockedBitmapPixels;
use super::jni_throwing;

/// Copies a validated software-bitmap crop into an Android hardware buffer.
#[jni_fn("com.hippo.ehviewer.image.ImageKt")]
pub fn copyBitmapToAHB(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    source_bitmap: jobject,
    hardware_buffer: jobject,
    x: jint,
    y: jint,
) {
    jni_throwing(&mut env, |env| {
        ensure!(!source_bitmap.is_null(), "source bitmap is null");
        ensure!(!hardware_buffer.is_null(), "hardware buffer is null");
        // SAFETY: non-null JNI arguments are strongly referenced for this
        // synchronous native call. Kotlin does not close either object until it
        // returns, and no native reference escapes the call.
        #[allow(
            unsafe_code,
            reason = "constructing NDK views requires live non-null raw JNI references"
        )]
        let source_bitmap = unsafe { Bitmap::from_jni(env.get_raw(), source_bitmap) };
        #[allow(
            unsafe_code,
            reason = "constructing NDK views requires live non-null raw JNI references"
        )]
        let hardware_buffer = unsafe { HardwareBuffer::from_jni(env.get_raw(), hardware_buffer) };

        let source_info = source_bitmap.info()?;
        let description = hardware_buffer.describe();
        ensure!(
            description.layers == 1,
            "hardware buffer must have one layer"
        );
        ensure!(
            description
                .usage
                .contains(HardwareBufferUsage::CPU_WRITE_RARELY),
            "hardware buffer is not CPU-writable"
        );
        let bytes_per_pixel = validate_formats(source_info.format(), description.format)?;
        let source_layout = ImageLayout::new(
            source_info.width(),
            source_info.height(),
            usize::try_from(source_info.stride())?,
            bytes_per_pixel,
        )?;
        let destination_stride = usize::try_from(description.stride)?
            .checked_mul(bytes_per_pixel)
            .context("hardware buffer row stride overflow")?;
        let destination_layout = ImageLayout::new(
            description.width,
            description.height,
            destination_stride,
            bytes_per_pixel,
        )?;
        let region = Region::new(
            u32::try_from(x).context("negative source x-coordinate")?,
            u32::try_from(y).context("negative source y-coordinate")?,
            description.width,
            description.height,
        )?;

        let source_pixels = LockedBitmapPixels::lock(&source_bitmap)?;
        let destination_pixels = LockedHardwareBufferPixels::lock(&hardware_buffer)?;
        let source_pointer = NonNull::new(source_pixels.pointer().cast::<u8>())
            .context("Android bitmap returned a null pixel address")?;
        let destination_pointer = NonNull::new(destination_pixels.pointer().cast::<u8>())
            .context("hardware buffer returned a null pixel address")?;
        ensure_disjoint(
            source_pointer,
            source_layout.required_len(),
            destination_pointer,
            destination_layout.required_len(),
        )?;
        // SAFETY: both NDK locks keep their non-null allocations stable. Layouts
        // validated the slice lengths, and ensure_disjoint established the
        // exclusive destination range required by `&mut [u8]`.
        #[allow(
            unsafe_code,
            reason = "both locked allocations have validated non-null disjoint ranges and bounded layouts"
        )]
        let result = unsafe {
            let source = from_raw_parts(source_pointer.as_ptr(), source_layout.required_len());
            let destination = from_raw_parts_mut(
                destination_pointer.as_ptr(),
                destination_layout.required_len(),
            );
            copy_to_origin(
                source,
                source_layout,
                destination,
                destination_layout,
                region,
            )
        };
        destination_pixels.unlock()?;
        source_pixels.unlock()?;
        result
    });
}

fn validate_formats(source: BitmapFormat, destination: HardwareBufferFormat) -> Result<usize> {
    let (expected, bytes_per_pixel) = match source {
        BitmapFormat::RGBA_8888 => (HardwareBufferFormat::R8G8B8A8_UNORM, 4),
        BitmapFormat::RGB_565 => (HardwareBufferFormat::R5G6B5_UNORM, 2),
        BitmapFormat::RGBA_F16 => (HardwareBufferFormat::R16G16B16A16_FLOAT, 8),
        _ => return Err(anyhow::anyhow!("unsupported source bitmap format")),
    };
    ensure!(
        destination == expected,
        "source bitmap and hardware buffer formats differ"
    );
    Ok(bytes_per_pixel)
}

fn ensure_disjoint(
    source: NonNull<u8>,
    source_len: usize,
    destination: NonNull<u8>,
    destination_len: usize,
) -> Result<()> {
    let source_start = source.as_ptr().addr();
    let destination_start = destination.as_ptr().addr();
    let source_end = source_start
        .checked_add(source_len)
        .context("source pointer range overflow")?;
    let destination_end = destination_start
        .checked_add(destination_len)
        .context("destination pointer range overflow")?;
    ensure!(
        source_end <= destination_start || destination_end <= source_start,
        "source and destination pixel ranges overlap"
    );
    Ok(())
}

struct LockedHardwareBufferPixels<'buffer> {
    buffer: &'buffer HardwareBuffer,
    pointer: *mut c_void,
    locked: bool,
}

impl<'buffer> LockedHardwareBufferPixels<'buffer> {
    fn lock(buffer: &'buffer HardwareBuffer) -> Result<Self> {
        Ok(Self {
            buffer,
            pointer: buffer.lock(HardwareBufferUsage::CPU_WRITE_RARELY, None, None)?,
            locked: true,
        })
    }

    fn pointer(&self) -> *mut c_void {
        self.pointer
    }

    fn unlock(mut self) -> Result<()> {
        self.buffer.unlock()?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for LockedHardwareBufferPixels<'_> {
    fn drop(&mut self) {
        if self.locked {
            let _ = self.buffer.unlock();
        }
    }
}
