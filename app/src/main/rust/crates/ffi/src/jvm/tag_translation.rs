//! Descriptor-backed immutable tag indexes and bounded V1 query transport.

use std::collections::HashMap;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context as _, Result, ensure};
use eh_domain::contracts::ApiEnvelope;
use eh_net::tag_translation::{API_VERSION, MAX_INPUT_BYTES, MODULE, SuggestionRequest, TagIndex};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass, JString};
use jni::sys::{jboolean, jbyteArray, jint, jlong, jstring};
use jni_fn::jni_fn;

use super::{decode_request, deref_direct_bytebuffer, duplicate_file_descriptor, jni_throwing};

static INDEXES: OnceLock<Mutex<HashMap<i64, Arc<TagIndex>>>> = OnceLock::new();
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
fn registry() -> &'static Mutex<HashMap<i64, Arc<TagIndex>>> {
    INDEXES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn insert(index: TagIndex) -> Result<i64> {
    let handle = NEXT_HANDLE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| anyhow::anyhow!("Tag index handles exhausted"))?;
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Tag index registry unavailable"))?
        .insert(handle, Arc::new(index));
    Ok(handle)
}
fn lookup(handle: i64) -> Result<Arc<TagIndex>> {
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Tag index registry unavailable"))?
        .get(&handle)
        .cloned()
        .context("Tag index is closed")
}
fn close(handle: i64) -> Result<()> {
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Tag index registry unavailable"))?
        .remove(&handle);
    Ok(())
}

/// Loads one bounded snapshot without modifying the Android descriptor's shared offset.
#[jni_fn("com.hippo.ehviewer.bridge.tags.TagTranslationBridgeKt")]
pub fn nativeOpenTagIndexV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    version: jint,
    descriptor: jint,
    remote: jboolean,
) -> jlong {
    jni_throwing(&mut env, |_| {
        open_descriptor(version, descriptor, remote != 0)
    })
}

fn open_descriptor(version: i32, descriptor: i32, remote: bool) -> Result<i64> {
    ensure!(
        version == i32::from(API_VERSION),
        "Unsupported tag index API version"
    );
    let file = duplicate_file_descriptor(descriptor, "tag index")?;
    let size = file.metadata()?.len();
    ensure!(
        size > 0 && size <= MAX_INPUT_BYTES,
        "Tag index input exceeds resource budget"
    );
    let mut bytes = vec![0; usize::try_from(size)?];
    file.read_exact_at(&mut bytes, 0)
        .context("Cannot read tag index snapshot")?;
    let index = TagIndex::parse(bytes.as_slice(), remote)
        .map_err(|_| anyhow::anyhow!("Invalid tag translation input"))?;
    insert(index)
}

/// Queries one index and returns an exact-sized CBOR envelope, never a request-size-multiplied buffer.
#[jni_fn("com.hippo.ehviewer.bridge.tags.TagTranslationBridgeKt")]
pub fn nativeSuggestTagsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    handle: jlong,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        let bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: SuggestionRequest = decode_request(bytes, request_length)?;
        let index = lookup(handle)?;
        let envelope = match index.suggest(&request) {
            Ok(rows) => ApiEnvelope::ok(API_VERSION, MODULE, rows),
            Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
        };
        let bytes = serde_cbor::to_vec(&envelope)?;
        ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "Tag suggestion response exceeds resource budget"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}

/// Looks up one translation while an Arc lease protects a concurrently retired native index.
#[jni_fn("com.hippo.ehviewer.bridge.tags.TagTranslationBridgeKt")]
pub fn nativeTranslateTagV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    handle: jlong,
    prefix: JString<'_>,
    tag: JString<'_>,
) -> jstring {
    jni_throwing(&mut env, |env| {
        let prefix: String = env.get_string(&prefix)?.into();
        let tag: String = env.get_string(&tag)?.into();
        let index = lookup(handle)?;
        match index.translation(&prefix, &tag) {
            Some(value) => {
                ensure!(
                    value.len() <= 64 * 1024,
                    "Tag translation exceeds resource budget"
                );
                Ok(env.new_string(value)?.into_raw())
            }
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// Idempotently retires one index; in-flight queries keep their independent Arc leases.
#[jni_fn("com.hippo.ehviewer.bridge.tags.TagTranslationBridgeKt")]
pub fn nativeCloseTagIndexV1(mut env: JNIEnv<'_>, _: JClass<'_>, handle: jlong) {
    jni_throwing(&mut env, |_| close(handle));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptor_snapshot_preserves_offset_and_rejects_version_size_and_json() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::fd::AsRawFd;
        let path = std::env::temp_dir().join(format!(
            "eh-tag-index-{}-{}.json",
            std::process::id(),
            NEXT_HANDLE.load(Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        std::fs::remove_file(&path).unwrap(); // POSIX unlinks immediately; fd remains usable, even after assertion failure.
        file.write_all(b"{\"f\":{\"aa\":\"hint\"}}").unwrap();
        file.seek(SeekFrom::Start(3)).unwrap();
        let handle = open_descriptor(1, file.as_raw_fd(), false).unwrap();
        assert_eq!(file.stream_position().unwrap(), 3);
        assert_eq!(lookup(handle).unwrap().translation("f", "aa"), Some("hint"));
        close(handle).unwrap();
        assert_eq!(
            open_descriptor(2, -1, false).unwrap_err().to_string(),
            "Unsupported tag index API version"
        );
        assert!(open_descriptor(1, -1, false).is_err());
        file.set_len(0).unwrap();
        assert!(open_descriptor(1, file.as_raw_fd(), false).is_err());
        file.set_len(MAX_INPUT_BYTES + 1).unwrap();
        assert!(open_descriptor(1, file.as_raw_fd(), false).is_err());
        file.set_len(0).unwrap();
        file.rewind().unwrap();
        file.write_all(b"bad").unwrap();
        assert_eq!(
            open_descriptor(1, file.as_raw_fd(), false)
                .unwrap_err()
                .to_string(),
            "Invalid tag translation input"
        );
    }
    #[test]
    fn unique_handles_lease_and_idempotent_close() {
        let first =
            insert(TagIndex::parse(b"{\"f\":{\"aa\":\"hint\"}}".as_slice(), false).unwrap())
                .unwrap();
        let second = insert(TagIndex::parse(b"{}".as_slice(), false).unwrap()).unwrap();
        assert_ne!(first, second);
        let lease = lookup(first).unwrap();
        close(first).unwrap();
        close(first).unwrap();
        assert!(lookup(first).is_err());
        assert_eq!(lease.translation("f", "aa"), Some("hint"));
        assert!(lookup(0).is_err());
        close(second).unwrap();
    }
}
