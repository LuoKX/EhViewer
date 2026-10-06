//! Animated WebP decoder ownership behind opaque synchronized handles.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::{Context, Result, anyhow, ensure};
use eh_image::webp::AnimationDecoder;
pub(super) use eh_image::webp::ImageInfo;
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
static DECODERS: OnceLock<Mutex<HashMap<i64, Decoder>>> = OnceLock::new();

struct Decoder {
    decoder: AnimationDecoder,
}
impl Decoder {
    fn new(data: &[u8]) -> Result<Self> {
        Ok(Self {
            decoder: AnimationDecoder::new(data)?,
        })
    }
    fn info(&self) -> Result<ImageInfo> {
        validate_info(self.decoder.info())
    }
    #[cfg(any(target_os = "android", test))]
    fn next_frame<R>(
        &mut self,
        reset: bool,
        consume: impl FnOnce(&[u8], ImageInfo, i32) -> Result<R>,
    ) -> Result<R> {
        self.decoder.next_frame(reset, consume)
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

fn validate_info(info: ImageInfo) -> Result<ImageInfo> {
    ensure!(
        info.width > 0 && info.height > 0,
        "animated WebP dimensions must be non-zero"
    );
    ensure!(info.frame_count > 0, "animated WebP has no frames");
    ensure!(
        info.width <= 0x00ff_ffff && info.height <= 0x00ff_ffff,
        "animated WebP dimensions exceed the packed JNI range"
    );
    ensure!(
        info.loop_count <= 0x0000_ffff,
        "animated WebP loop count exceeds the packed JNI range"
    );
    eh_image::webp::frame_length(info.width, info.height)?;
    Ok(info)
}

#[cfg(test)]
fn frame_length(info: ImageInfo) -> Result<usize> {
    eh_image::webp::frame_length(info.width, info.height)
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
    ) -> ImageInfo {
        image_info(canvas_width, canvas_height, loop_count, frame_count)
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
