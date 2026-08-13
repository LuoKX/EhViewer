//! JNI adapters for the Rust-owned GIF delay workaround.

use std::fs::File;
use std::os::unix::fs::FileExt;

use anyhow::{Context as _, Result, ensure};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jbyteArray, jint};
use jni_fn::jni_fn;

use super::{deref_mut_direct_bytebuffer, jni_throwing};

const GIF_HEADER_BYTES: usize = 6;
const MAX_GIF_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// Snapshots and rewrites a GIF without changing the Java descriptor position.
///
/// Non-GIF input is classified from only its first six bytes and returns null.
/// A non-seekable descriptor also returns null without consuming it, allowing
/// Android to reopen the source through the normal decoder. GIF input is bounded
/// to the repository's maximum supported image size before a Rust allocation is
/// attempted, then copied to a Java-owned byte array.
#[jni_fn("com.hippo.ehviewer.jni.GifUtilsKt")]
pub fn prepareGifSource(mut env: JNIEnv<'_>, _: JClass<'_>, file_descriptor: jint) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        let file = super::duplicate_file_descriptor(file_descriptor, "GIF source")?;
        match prepare_gif_source(&file, MAX_GIF_SOURCE_BYTES)? {
            Some(source) => Ok(env.byte_array_from_slice(&source)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// Rewrites one caller-owned direct buffer synchronously in place.
///
/// Null and heap buffers produce a contained Java `RuntimeException`.
#[jni_fn("com.hippo.ehviewer.jni.GifUtilsKt")]
pub fn rewriteGifSource(mut env: JNIEnv<'_>, _: JClass<'_>, buffer: JByteBuffer<'_>) {
    jni_throwing(&mut env, |env| {
        let source = deref_mut_direct_bytebuffer(env, &buffer)?;
        let _ = eh_image::gif::rewrite_short_frame_delays(source);
        Ok(())
    });
}

fn prepare_gif_source(file: &File, maximum_bytes: u64) -> Result<Option<Vec<u8>>> {
    let mut header = [0_u8; GIF_HEADER_BYTES];
    let header_bytes = match read_at(file, &mut header, 0) {
        Ok(bytes) => bytes,
        Err(error) if error.raw_os_error() == Some(libc::ESPIPE) => return Ok(None),
        Err(error) => return Err(error).context("Failed to read GIF source header"),
    };
    if header_bytes < GIF_HEADER_BYTES || !eh_image::gif::is_gif(&header) {
        return Ok(None);
    }

    let metadata_before = file.metadata().context("Failed to read GIF source size")?;
    let source_bytes = metadata_before.len();
    let modified_before = metadata_before.modified().ok();
    ensure!(
        source_bytes <= maximum_bytes,
        "GIF source exceeds the supported byte limit"
    );
    let source_bytes =
        usize::try_from(source_bytes).context("GIF source size exceeds Rust range")?;
    let mut source = Vec::new();
    source
        .try_reserve_exact(source_bytes)
        .context("Failed to reserve bounded GIF source bytes")?;
    source.resize(source_bytes, 0);
    let copied = read_at(file, &mut source, 0).context("Failed to snapshot GIF source")?;
    ensure!(
        copied == source_bytes,
        "GIF source changed while it was being prepared"
    );
    ensure!(
        eh_image::gif::is_gif(&source),
        "GIF source changed while it was being prepared"
    );
    let source_offset = u64::try_from(source_bytes)?;
    let mut trailing_byte = [0_u8; 1];
    ensure!(
        read_at(file, &mut trailing_byte, source_offset)? == 0,
        "GIF source changed while it was being prepared"
    );
    let metadata_after = file
        .metadata()
        .context("Failed to recheck GIF source size")?;
    ensure!(
        metadata_after.len() == source_offset,
        "GIF source changed while it was being prepared"
    );
    if let (Some(before), Ok(after)) = (modified_before, metadata_after.modified()) {
        ensure!(
            before == after,
            "GIF source changed while it was being prepared"
        );
    }
    let _ = eh_image::gif::rewrite_short_frame_delays(&mut source);
    Ok(Some(source))
}

fn read_at(file: &File, mut destination: &mut [u8], mut offset: u64) -> std::io::Result<usize> {
    let expected = destination.len();
    while !destination.is_empty() {
        let read_bytes = file.read_at(destination, offset)?;
        if read_bytes == 0 {
            break;
        }
        let read_offset = u64::try_from(read_bytes)
            .map_err(|_| std::io::Error::other("GIF source read count exceeds u64"))?;
        offset = offset
            .checked_add(read_offset)
            .ok_or_else(|| std::io::Error::other("GIF source offset overflow"))?;
        destination = &mut destination[read_bytes..];
    }
    Ok(expected - destination.len())
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    const FAST_GIF: &[u8] = b"GIF89a\0\x21\xf9\x04\0\x01\0\0\0tail";
    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn snapshots_rewrites_and_repeats_without_mutating_source_or_position() {
        let mut file = fixture_file(FAST_GIF);
        file.seek(SeekFrom::Start(3)).unwrap();

        let first = prepare_gif_source(&file, 1024).unwrap().unwrap();
        let second = prepare_gif_source(&file, 1024).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(&first[11..13], &[10, 0]);
        assert_eq!(file.stream_position().unwrap(), 3);

        let mut original = vec![0; FAST_GIF.len()];
        assert_eq!(read_at(&file, &mut original, 0).unwrap(), FAST_GIF.len());
        assert_eq!(original, FAST_GIF);
        assert_eq!(file.stream_position().unwrap(), 3);
    }

    #[test]
    fn non_gif_and_short_inputs_return_none_before_the_body_limit_matters() {
        let mut non_gif = fixture_file(&[b'N'; 64]);
        non_gif.seek(SeekFrom::Start(7)).unwrap();
        assert_eq!(prepare_gif_source(&non_gif, 6).unwrap(), None);
        assert_eq!(non_gif.stream_position().unwrap(), 7);

        assert_eq!(
            prepare_gif_source(&fixture_file(b"GIF89"), 4).unwrap(),
            None
        );
    }

    #[test]
    fn non_seekable_sources_return_none_without_consuming_their_header() {
        let (mut writer, reader) = UnixStream::pair().unwrap();
        writer.write_all(FAST_GIF).unwrap();
        let reader = File::from(OwnedFd::from(reader));

        assert_eq!(prepare_gif_source(&reader, 1024).unwrap(), None);
        let mut unchanged = vec![0; FAST_GIF.len()];
        (&reader).read_exact(&mut unchanged).unwrap();
        assert_eq!(unchanged, FAST_GIF);
    }

    #[test]
    fn gif_limit_is_rejected_and_a_header_only_gif_remains_safe() {
        let error = prepare_gif_source(&fixture_file(FAST_GIF), 6).unwrap_err();
        assert!(error.to_string().contains("supported byte limit"));

        let header_only = fixture_file(b"GIF89a");
        let prepared = prepare_gif_source(&header_only, 1024).unwrap().unwrap();
        assert_eq!(prepared, b"GIF89a");
    }

    fn fixture_file(bytes: &[u8]) -> File {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ehviewer-gif-source-{}-{id}.fixture",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        std::fs::remove_file(path).unwrap();
        file
    }
}
