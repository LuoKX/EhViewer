//! Animated WebP decoder ownership behind opaque synchronized handles.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::ptr::NonNull;
#[cfg(any(target_os = "android", test))]
use std::ptr::null_mut;
#[cfg(any(target_os = "android", test))]
use std::slice::from_raw_parts;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::{Context, Result, anyhow, ensure};
#[cfg(target_os = "android")]
use webp_android as webp_sys;
#[cfg(not(target_os = "android"))]
use webp_host as webp_sys;
use webp_sys::{
    MODE_rgbA, WebPAnimDecoder, WebPAnimDecoderDelete, WebPAnimDecoderGetInfo, WebPAnimDecoderNew,
    WebPAnimDecoderOptions, WebPAnimInfo, WebPData,
};
#[cfg(any(target_os = "android", test))]
use webp_sys::{WebPAnimDecoderGetNext, WebPAnimDecoderHasMoreFrames, WebPAnimDecoderReset};

static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
static DECODERS: OnceLock<Mutex<HashMap<i64, Decoder>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ImageInfo {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) loop_count: u32,
    pub(super) frame_count: u32,
}

struct Decoder {
    decoder: NonNull<WebPAnimDecoder>,
    // libwebp borrows WebPData bytes for the complete decoder lifetime.
    _data: Vec<u8>,
}

// SAFETY: libwebp animation decoders have no thread affinity. Every operation
// is serialized by DECODERS, and the backing bytes remain immutable in Decoder.
#[allow(
    unsafe_code,
    reason = "the synchronized registry transfers libwebp decoder ownership across JNI threads"
)]
unsafe impl Send for Decoder {}

impl Decoder {
    fn new(data: &[u8]) -> Result<Self> {
        ensure!(!data.is_empty(), "animated WebP input is empty");
        let data = data.to_vec();
        let webp_data = WebPData {
            bytes: data.as_ptr(),
            size: data.len(),
        };
        let options = WebPAnimDecoderOptions {
            color_mode: MODE_rgbA,
            use_threads: 1,
            padding: [0; 7],
        };
        // SAFETY: webp_data points to immutable Vec storage retained by Self;
        // options is fully initialized for libwebp's animation decoder API.
        #[allow(
            unsafe_code,
            reason = "libwebp receives initialized options and bytes retained by the decoder owner"
        )]
        let decoder =
            NonNull::new(unsafe { WebPAnimDecoderNew(&raw const webp_data, &raw const options) })
                .context("libwebp rejected the animated image")?;
        let decoder = Self {
            decoder,
            _data: data,
        };
        decoder.info()?;
        Ok(decoder)
    }

    fn info(&self) -> Result<ImageInfo> {
        let mut info = WebPAnimInfo {
            canvas_width: 0,
            canvas_height: 0,
            loop_count: 0,
            bgcolor: 0,
            frame_count: 0,
            pad: [0; 4],
        };
        // SAFETY: self.decoder remains valid until Drop and info is writable.
        #[allow(
            unsafe_code,
            reason = "the owned decoder is live and the metadata output is initialized writable storage"
        )]
        let succeeded = unsafe { WebPAnimDecoderGetInfo(self.decoder.as_ptr(), &raw mut info) };
        ensure!(succeeded != 0, "libwebp could not read animation metadata");
        validate_info(info)
    }

    #[cfg(any(target_os = "android", test))]
    fn next_frame<R>(
        &mut self,
        reset: bool,
        consume: impl FnOnce(&[u8], ImageInfo, i32) -> Result<R>,
    ) -> Result<R> {
        let info = self.info()?;
        // SAFETY: the decoder is valid and exclusively borrowed through the
        // registry mutex for this complete operation.
        #[allow(
            unsafe_code,
            reason = "the registry mutex gives this call exclusive access to the live decoder"
        )]
        unsafe {
            if reset || WebPAnimDecoderHasMoreFrames(self.decoder.as_ptr()) == 0 {
                WebPAnimDecoderReset(self.decoder.as_ptr());
            }
        }
        let mut pixels = null_mut();
        let mut timestamp = 0;
        // SAFETY: libwebp initializes pixels/timestamp on success; the returned
        // canvas remains valid until the next decoder operation, which cannot
        // occur while the registry mutex and this mutable borrow are held.
        #[allow(
            unsafe_code,
            reason = "the live decoder exclusively initializes the frame pointer and timestamp outputs"
        )]
        let succeeded = unsafe {
            WebPAnimDecoderGetNext(self.decoder.as_ptr(), &raw mut pixels, &raw mut timestamp)
        };
        ensure!(succeeded != 0, "libwebp could not decode the next frame");
        let pixels = NonNull::new(pixels).context("libwebp returned an empty frame")?;
        ensure!(timestamp > 0, "libwebp returned an invalid frame timestamp");
        let frame_len = frame_length(info)?;
        // SAFETY: successful WebPAnimDecoderGetNext returns a complete RGBA
        // canvas with frame_len bytes, valid until the next decoder operation.
        #[allow(
            unsafe_code,
            reason = "libwebp returned a complete RGBA canvas whose checked length remains live under the mutex"
        )]
        let frame = unsafe { from_raw_parts(pixels.as_ptr(), frame_len) };
        consume(frame, info, timestamp)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: Decoder owns this non-null pointer and Drop runs exactly once.
        #[allow(
            unsafe_code,
            reason = "Decoder uniquely owns this non-null libwebp pointer until its single drop"
        )]
        unsafe {
            WebPAnimDecoderDelete(self.decoder.as_ptr());
        }
    }
}

pub(super) fn create_decoder(data: &[u8]) -> Result<i64> {
    let decoder = Decoder::new(data)?;
    let handle = next_handle(&NEXT_HANDLE)?;
    match lock_decoders()?.entry(handle) {
        Entry::Vacant(entry) => {
            entry.insert(decoder);
        }
        Entry::Occupied(_) => return Err(anyhow!("animated WebP handle collision")),
    }
    Ok(handle)
}

fn next_handle(counter: &AtomicI64) -> Result<i64> {
    let handle = counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| anyhow!("animated WebP handle space is exhausted"))?;
    ensure!(handle > 0, "animated WebP handle is invalid");
    Ok(handle)
}

pub(super) fn get_image_info(handle: i64) -> Result<ImageInfo> {
    lock_decoders()?
        .get(&handle)
        .with_context(|| format!("unknown animated WebP handle {handle}"))?
        .info()
}

pub(super) fn pack_image_info(info: ImageInfo) -> u64 {
    u64::from(info.width) << 40 | u64::from(info.height) << 16 | u64::from(info.loop_count)
}

#[cfg(any(target_os = "android", test))]
pub(super) fn decode_next_frame<R>(
    handle: i64,
    reset: bool,
    consume: impl FnOnce(&[u8], ImageInfo, i32) -> Result<R>,
) -> Result<R> {
    lock_decoders()?
        .get_mut(&handle)
        .with_context(|| format!("unknown animated WebP handle {handle}"))?
        .next_frame(reset, consume)
}

pub(super) fn destroy_decoder(handle: i64) -> Result<bool> {
    if handle <= 0 {
        return Ok(false);
    }
    Ok(lock_decoders()?.remove(&handle).is_some())
}

fn validate_info(info: WebPAnimInfo) -> Result<ImageInfo> {
    ensure!(
        info.canvas_width > 0 && info.canvas_height > 0,
        "animated WebP dimensions must be non-zero"
    );
    ensure!(info.frame_count > 0, "animated WebP has no frames");
    ensure!(
        info.canvas_width <= 0x00ff_ffff && info.canvas_height <= 0x00ff_ffff,
        "animated WebP dimensions exceed the packed JNI range"
    );
    ensure!(
        info.loop_count <= 0x0000_ffff,
        "animated WebP loop count exceeds the packed JNI range"
    );
    Ok(ImageInfo {
        width: info.canvas_width,
        height: info.canvas_height,
        loop_count: info.loop_count,
        frame_count: info.frame_count,
    })
}

#[cfg(any(target_os = "android", test))]
fn frame_length(info: ImageInfo) -> Result<usize> {
    usize::try_from(info.width)
        .context("animated WebP width exceeds this platform")?
        .checked_mul(
            usize::try_from(info.height).context("animated WebP height exceeds this platform")?,
        )
        .and_then(|pixels| pixels.checked_mul(4))
        .context("animated WebP frame size overflow")
}

fn lock_decoders() -> Result<MutexGuard<'static, HashMap<i64, Decoder>>> {
    lock_decoder_registry(DECODERS.get_or_init(|| Mutex::new(HashMap::new())))
}

fn lock_decoder_registry(
    registry: &'static Mutex<HashMap<i64, Decoder>>,
) -> Result<MutexGuard<'static, HashMap<i64, Decoder>>> {
    registry
        .lock()
        .map_err(|_| anyhow!("animated WebP decoder registry is poisoned"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicI64;

    use super::webp_sys::WebPAnimInfo;
    use super::{
        ImageInfo, create_decoder, decode_next_frame, destroy_decoder, frame_length,
        get_image_info, lock_decoder_registry, next_handle, pack_image_info, validate_info,
    };

    const ANIMATED_WEBP_HEX: &str = include_str!("../../../test_data/animated.webp.hex");

    #[test]
    fn owns_input_reports_metadata_and_decodes_all_frames() {
        let mut data = decode_hex(ANIMATED_WEBP_HEX);
        let handle = create_decoder(&data).expect("the checked-in animation should open");
        data.fill(0);

        let info = get_image_info(handle).expect("animation metadata should be available");
        assert_eq!((info.width, info.height), (400, 400));
        assert_eq!(info.frame_count, 10);
        assert_eq!(info.loop_count, 0);
        let packed = pack_image_info(info);
        assert_eq!(packed >> 40, 400);
        assert_eq!(packed >> 16 & 0x00ff_ffff, 400);
        assert_eq!(packed & 0xffff, 0);

        let mut timestamps = Vec::new();
        for _ in 0..10 {
            let timestamp = decode_next_frame(handle, false, |frame, frame_info, timestamp| {
                assert_eq!(frame.len(), 400 * 400 * 4);
                assert_eq!(frame_info, info);
                Ok(timestamp)
            })
            .expect("every checked-in frame should decode");
            timestamps.push(timestamp);
        }
        assert_eq!(timestamps, (40..=400).step_by(40).collect::<Vec<_>>());
        assert_eq!(
            decode_next_frame(handle, false, |_, _, timestamp| Ok(timestamp))
                .expect("an exhausted decoder should restart"),
            40
        );
        assert_eq!(
            decode_next_frame(handle, true, |_, _, timestamp| Ok(timestamp))
                .expect("an explicit reset should restart"),
            40
        );
        assert!(destroy_decoder(handle).expect("destroy should lock the registry"));
        assert!(!destroy_decoder(handle).expect("destroy should be idempotent"));
    }

    #[test]
    fn rejects_invalid_data_and_unknown_handles() {
        assert!(create_decoder(&[]).is_err());
        assert!(create_decoder(b"not a webp").is_err());
        assert!(get_image_info(i64::MAX).is_err());
        let consumer = |_: &[u8], _: ImageInfo, _: i32| Ok(());
        consumer(&[], image_info(1, 1, 0, 1), 1).expect("the test consumer is infallible");
        assert!(
            decode_next_frame(i64::MAX, false, consumer)
                .expect_err("an unknown handle must be rejected")
                .to_string()
                .contains("unknown")
        );
        assert!(!destroy_decoder(0).expect("zero destroy is a no-op"));
        assert!(!destroy_decoder(i64::MAX).expect("unknown destroy is a no-op"));
    }

    #[test]
    fn callback_failure_preserves_independent_decoder_handles() {
        let data = decode_hex(ANIMATED_WEBP_HEX);
        let first = create_decoder(&data).expect("the first animation should open");
        let second = create_decoder(&data).expect("the second animation should open");

        let callback_error = decode_next_frame(first, false, |_, _, _| {
            Err::<(), _>(anyhow::anyhow!("isolated callback failure"))
        })
        .expect_err("callback errors must be returned to the boundary");
        assert_eq!(callback_error.to_string(), "isolated callback failure");
        assert_eq!(
            decode_next_frame(first, false, |_, _, timestamp| Ok(timestamp))
                .expect("the first decoder should remain registered"),
            80
        );
        assert_eq!(
            decode_next_frame(second, false, |_, _, timestamp| Ok(timestamp))
                .expect("the second decoder should remain independent"),
            40
        );

        assert!(destroy_decoder(first).expect("the first decoder should be removable"));
        assert!(destroy_decoder(second).expect("the second decoder should be removable"));
    }

    #[test]
    fn validates_metadata_sizes_handles_and_poisoned_registries() {
        assert!(validate_info(webp_info(0, 1, 0, 1)).is_err());
        assert!(validate_info(webp_info(1, 0, 0, 1)).is_err());
        assert!(validate_info(webp_info(1, 1, 0, 0)).is_err());
        assert!(validate_info(webp_info(0x0100_0000, 1, 0, 1)).is_err());
        assert!(validate_info(webp_info(1, 0x0100_0000, 0, 1)).is_err());
        assert!(validate_info(webp_info(1, 1, 0x0001_0000, 1)).is_err());
        assert!(frame_length(image_info(u32::MAX, u32::MAX, 0, 1)).is_err());

        assert!(next_handle(&AtomicI64::new(i64::MAX)).is_err());
        assert!(next_handle(&AtomicI64::new(0)).is_err());

        let registry: &'static Mutex<HashMap<i64, super::Decoder>> =
            Box::leak(Box::new(Mutex::new(HashMap::new())));
        let poisoner = std::thread::spawn(move || {
            let _guard = registry.lock().expect("fresh registry should lock");
            panic!("deliberately poison the isolated test registry");
        });
        assert!(poisoner.join().is_err());
        assert!(lock_decoder_registry(registry).is_err());
    }

    fn image_info(width: u32, height: u32, loop_count: u32, frame_count: u32) -> ImageInfo {
        ImageInfo {
            width,
            height,
            loop_count,
            frame_count,
        }
    }

    fn webp_info(
        canvas_width: u32,
        canvas_height: u32,
        loop_count: u32,
        frame_count: u32,
    ) -> WebPAnimInfo {
        WebPAnimInfo {
            canvas_width,
            canvas_height,
            loop_count,
            bgcolor: 0,
            frame_count,
            pad: [0; 4],
        }
    }

    fn decode_hex(input: &str) -> Vec<u8> {
        let digits = input
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        assert_eq!(digits.len() % 2, 0, "hex fixture must contain byte pairs");
        digits
            .chunks_exact(2)
            .map(|pair| {
                let pair = std::str::from_utf8(pair).expect("hex fixture should be ASCII");
                u8::from_str_radix(pair, 16).expect("hex fixture should contain valid digits")
            })
            .collect()
    }
}
