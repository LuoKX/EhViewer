//! Android Bitmap adapters for host-testable image algorithms and animated WebP.

use std::ffi::c_void;
use std::mem::align_of;
use std::ptr::copy_nonoverlapping;
use std::slice::from_raw_parts;

use anyhow::{Context as _, Result, anyhow, ensure};
use eh_image::border::DetectBorder;
use eh_image::crop_policy::plan_crop;
use eh_image::pixel::{CustomPixel, ImageConsumer, Rgb565, Rgba8888, RgbaF16};
use eh_image::qr::DetectQrCode;
use image::ImageBuffer;
use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jboolean, jint, jintArray, jlong, jobject};
use jni_fn::jni_fn;
use ndk::bitmap::{Bitmap, BitmapFormat};

use super::jni_throwing;
use crate::webp::decode_next_frame;

/// Detects the visible border of one supported Android Bitmap.
///
/// The returned array contains the four Kotlin-compatible crop coordinates.
/// Null, empty, malformed, or unsupported bitmaps produce a contained Java
/// `RuntimeException` and are unlocked before control returns to Java.
#[jni_fn("com.hippo.ehviewer.image.ImageKt")]
pub fn detectBorder(mut env: JNIEnv<'_>, _class: JClass<'_>, object: jobject) -> jintArray {
    jni_throwing(&mut env, |env| {
        let slice = use_bitmap_content(env, object, DetectBorder)?;
        let array = env.new_int_array(4)?;
        env.set_int_array_region(&array, 0, &slice)?;
        Ok(array.into_raw())
    })
}

/// Applies the original crop policy and pixel detector in one Android call.
/// Returns null to keep the original image; exceptions are never a fallback signal.
#[jni_fn("com.hippo.ehviewer.image.ImageKt")]
pub fn detectCropV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    object: jobject,
    has_qr_code: jboolean,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let rect = plan_crop(
            has_qr_code != 0,
            || {
                ensure!(!object.is_null(), "Crop bitmap is null");
                // SAFETY: the synchronous call borrows a live Java Bitmap; it never escapes.
                #[allow(
                    unsafe_code,
                    reason = "NDK metadata requires a live borrowed JNI Bitmap"
                )]
                let bitmap = unsafe { Bitmap::from_jni(env.get_raw(), object) };
                let info = bitmap.info()?;
                Ok((i32::try_from(info.width())?, i32::try_from(info.height())?))
            },
            || use_bitmap_content(env, object, DetectBorder),
        )?;
        let Some(rect) = rect else {
            return Ok(std::ptr::null_mut());
        };
        let array = env.new_int_array(4)?;
        env.set_int_array_region(&array, 0, &rect)?;
        Ok(array.into_raw())
    })
}

/// Reports whether one supported Android Bitmap contains a QR finder layout.
///
/// Null or unsupported bitmaps produce a contained Java `RuntimeException` and
/// every successful native lock is released before returning.
#[jni_fn("com.hippo.ehviewer.image.ImageKt")]
pub fn hasQrCode(mut env: JNIEnv<'_>, _class: JClass<'_>, object: jobject) -> jboolean {
    jni_throwing(&mut env, |env| {
        Ok(jboolean::from(use_bitmap_content(
            env,
            object,
            DetectQrCode,
        )?))
    })
}

/// Decodes one animated WebP canvas into a validated Android RGBA bitmap.
#[jni_fn("com.hippo.ehviewer.coil.AnimatedWebPDrawableKt")]
pub fn nativeDecodeNextFrame(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    decoder: jlong,
    reset: jboolean,
    bitmap: jobject,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(!bitmap.is_null(), "animated WebP target bitmap is null");
        decode_next_frame(decoder, reset != 0, |frame, info, timestamp| {
            // SAFETY: the validated JNI local reference remains live for this
            // synchronous call and the Bitmap wrapper never escapes it.
            #[allow(
                unsafe_code,
                reason = "constructing an NDK Bitmap view requires a live raw JNI reference"
            )]
            let handle = unsafe { Bitmap::from_jni(env.get_raw(), bitmap) };
            let bitmap_info = handle.info()?;
            ensure!(
                bitmap_info.format() == BitmapFormat::RGBA_8888,
                "animated WebP target bitmap must use RGBA_8888"
            );
            ensure!(
                bitmap_info.width() == info.width && bitmap_info.height() == info.height,
                "animated WebP target bitmap dimensions do not match the decoder"
            );
            let row_bytes = usize::try_from(info.width)?
                .checked_mul(4)
                .context("animated WebP row size overflow")?;
            let stride = usize::try_from(bitmap_info.stride())?;
            ensure!(
                stride >= row_bytes,
                "animated WebP bitmap stride is too small"
            );
            let height = usize::try_from(info.height)?;
            let last_row_offset = height
                .saturating_sub(1)
                .checked_mul(stride)
                .context("animated WebP bitmap address range overflow")?;
            last_row_offset
                .checked_add(row_bytes)
                .context("animated WebP bitmap address range overflow")?;
            let pixels = LockedBitmapPixels::lock(&handle)?;
            let dst = pixels.pointer().cast::<u8>();
            for row in 0..height {
                // SAFETY: frame contains width*height RGBA bytes; bitmap width,
                // height, format, and stride were validated before locking.
                #[allow(
                    unsafe_code,
                    reason = "validated RGBA frame and Bitmap row ranges are live, sized, and disjoint"
                )]
                unsafe {
                    copy_nonoverlapping(
                        frame.as_ptr().add(row * row_bytes),
                        dst.add(row * stride),
                        row_bytes,
                    );
                }
            }
            pixels.unlock()?;
            Ok(timestamp)
        })
    })
}

fn apply_bitmap_content<P, R, F>(
    ptr: *const u8,
    width: u32,
    height: u32,
    stride: u32,
    consumer: F,
) -> Result<R>
where
    P: CustomPixel,
    F: ImageConsumer<R>,
{
    let row_elements = usize::try_from(width)?
        .checked_mul(usize::from(P::CHANNEL_COUNT))
        .context("Android bitmap row element count overflow")?;
    let row_bytes = row_elements
        .checked_mul(std::mem::size_of::<P::Subpixel>())
        .context("Android bitmap row size overflow")?;
    let height_usize = usize::try_from(height)?;
    let size = row_elements
        .checked_mul(height_usize)
        .context("Android bitmap element count overflow")?;
    row_bytes
        .checked_mul(height_usize)
        .context("Android bitmap packed size overflow")?;
    let stride = usize::try_from(stride)?;
    ensure!(stride >= row_bytes, "Android bitmap stride is too small");
    let last_row_offset = height_usize
        .saturating_sub(1)
        .checked_mul(stride)
        .context("Android bitmap address range overflow")?;
    last_row_offset
        .checked_add(row_bytes)
        .context("Android bitmap address range overflow")?;

    if stride == row_bytes {
        ensure!(
            ptr.addr().is_multiple_of(align_of::<P::Subpixel>()),
            "Android bitmap pixels are misaligned"
        );
        // SAFETY: Android keeps the locked storage alive; the checked dimensions
        // and tight stride describe exactly `size` aligned subpixels.
        #[allow(
            unsafe_code,
            reason = "the locked tight Bitmap range is aligned and validated for the complete subpixel slice"
        )]
        let buffer = unsafe { from_raw_parts(ptr.cast::<P::Subpixel>(), size) };
        let image = ImageBuffer::<P, _>::from_raw(width, height, buffer)
            .context("Android bitmap dimensions do not match storage")?;
        return consumer.apply(&image);
    }

    let mut packed = vec![<P::Subpixel as image::Primitive>::DEFAULT_MIN_VALUE; size];
    for row in 0..height_usize {
        let src_offset = row * stride;
        let dst_offset = row * row_bytes;
        // SAFETY: source row bounds follow from the checked stride span; the
        // aligned Vec owns `size` initialized subpixels and each row writes a
        // disjoint `row_bytes` region within that allocation.
        #[allow(
            unsafe_code,
            reason = "validated Bitmap and owned packed rows are live, sized, aligned, and disjoint"
        )]
        unsafe {
            copy_nonoverlapping(
                ptr.add(src_offset),
                packed.as_mut_ptr().cast::<u8>().add(dst_offset),
                row_bytes,
            );
        }
    }
    let image = ImageBuffer::<P, _>::from_raw(width, height, packed.as_slice())
        .context("Android bitmap dimensions do not match packed storage")?;
    consumer.apply(&image)
}

pub(super) struct LockedBitmapPixels<'bitmap> {
    bitmap: &'bitmap Bitmap,
    pointer: *mut c_void,
    locked: bool,
}

impl<'bitmap> LockedBitmapPixels<'bitmap> {
    pub(super) fn lock(bitmap: &'bitmap Bitmap) -> Result<Self> {
        Ok(Self {
            bitmap,
            pointer: bitmap.lock_pixels()?,
            locked: true,
        })
    }

    pub(super) fn pointer(&self) -> *mut c_void {
        self.pointer
    }

    pub(super) fn unlock(mut self) -> Result<()> {
        self.bitmap.unlock_pixels()?;
        self.locked = false;
        Ok(())
    }
}

impl Drop for LockedBitmapPixels<'_> {
    fn drop(&mut self) {
        if self.locked {
            let _ = self.bitmap.unlock_pixels();
        }
    }
}

fn use_bitmap_content<R, F: ImageConsumer<R>>(
    env: &JNIEnv<'_>,
    bitmap: jobject,
    consumer: F,
) -> Result<R> {
    ensure!(!bitmap.is_null(), "Android bitmap is null");
    // SAFETY: the non-null JNI local reference remains alive throughout this
    // synchronous call, and no Bitmap wrapper escapes it.
    #[allow(
        unsafe_code,
        reason = "constructing an NDK Bitmap view requires a live raw JNI reference"
    )]
    let handle = unsafe { Bitmap::from_jni(env.get_raw(), bitmap) };

    let info = handle.info()?;
    let (width, height, stride, format) =
        (info.width(), info.height(), info.stride(), info.format());
    let pixels = LockedBitmapPixels::lock(&handle)?;
    let ptr = pixels.pointer().cast_const().cast::<u8>();
    let result = match format {
        BitmapFormat::RGBA_8888 => {
            apply_bitmap_content::<Rgba8888, _, _>(ptr, width, height, stride, consumer)
        }
        BitmapFormat::RGB_565 => {
            apply_bitmap_content::<Rgb565, _, _>(ptr, width, height, stride, consumer)
        }
        BitmapFormat::RGBA_F16 => {
            apply_bitmap_content::<RgbaF16, _, _>(ptr, width, height, stride, consumer)
        }
        _ => Err(anyhow!("Unsupported bitmap format")),
    };
    pixels.unlock()?;
    result
}
