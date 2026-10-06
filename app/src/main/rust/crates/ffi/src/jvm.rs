//! JVM direct-buffer transport and exported V1 adapters.

mod archive;
mod bbcode;
#[cfg(all(feature = "android", target_os = "android"))]
mod bitmap;
mod cache_keys;
mod comic_info;
mod comic_info_policy;
mod file_rules;
mod gallery_enrichment;
mod gallery_filter;
mod gallery_network_policy;
mod gallery_request;
mod gallery_title;
mod gif;
#[cfg(all(feature = "android-26", target_os = "android"))]
mod hardware_buffer;
mod hash;
mod health;
mod local_pages;
mod parser;
mod plan;
mod reader_policy;
mod recovery_eh_lookup;
mod recovery_metadata;
mod recovery_scan;
mod recovery_search;
mod spider_info;
mod storage_rules;
mod tag_translation;
mod text_spans;
mod transfer_speed;
mod webp;

use std::fs::File;
use std::os::fd::FromRawFd;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::slice::{from_raw_parts, from_raw_parts_mut};

use anyhow::{Context as _, Result, ensure};
use jni::JNIEnv;
use jni::objects::JByteBuffer;
use jni::sys::{jboolean, jint, jobject};
use serde::{Serialize, de::DeserializeOwned};

use crate::codec;

/// Duplicates a non-negative JVM-owned POSIX descriptor into one Rust-owned file.
///
/// The duplicate is closed when the returned file is dropped. POSIX duplicates
/// share an open-file description, so callers that use position-changing I/O
/// must restore the original position before returning to Java. Prefer
/// offset-based reads when the Java descriptor position must remain unchanged.
///
/// # Errors
///
/// Returns an error for a negative descriptor or a failed `dup` call.
#[allow(
    unsafe_code,
    reason = "duplicating a JVM-owned POSIX descriptor requires libc ownership conversion"
)]
fn duplicate_file_descriptor(file_descriptor: jint, description: &'static str) -> Result<File> {
    ensure!(
        file_descriptor >= 0,
        "{description} file descriptor is negative"
    );
    // SAFETY: `dup` does not take ownership and reports an invalid descriptor via `-1`.
    let duplicated_descriptor = unsafe { libc::dup(file_descriptor) };
    if duplicated_descriptor < 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("Failed to duplicate {description} file descriptor"));
    }
    // SAFETY: the successful `dup` result is uniquely owned by this `File`.
    Ok(unsafe { File::from_raw_fd(duplicated_descriptor) })
}

/// Borrows an immutable slice from a direct Java byte buffer for one synchronous call.
///
/// Null and non-direct buffers are rejected by JNI. The caller must copy or fully
/// consume the slice before returning to Java.
///
/// # Errors
///
/// Returns an error when the object is null, is not a direct buffer, or JNI cannot
/// report its address or capacity.
pub fn deref_direct_bytebuffer<'local>(
    env: &JNIEnv<'_>,
    buffer: &JByteBuffer<'local>,
) -> Result<&'local [u8]> {
    let ptr = env.get_direct_buffer_address(buffer)?;
    let capacity = env.get_direct_buffer_capacity(buffer)?;

    // SAFETY: `jni` rejects a null/non-direct buffer and a null address before
    // returning `ptr`; it also rejects JNI's `-1` capacity sentinel. The Java
    // call is synchronous, and this immutable view is copied before returning.
    #[allow(unsafe_code)]
    Ok(unsafe { from_raw_parts(ptr.cast_const(), capacity) })
}

/// Borrows a mutable slice from a direct Java byte buffer for one synchronous call.
///
/// The caller must not access the Java buffer concurrently while the returned
/// slice is live. Null and non-direct buffers are rejected by JNI.
///
/// # Errors
///
/// Returns an error when the object is null, is not a direct buffer, or JNI cannot
/// report its address or capacity.
pub fn deref_mut_direct_bytebuffer<'local>(
    env: &JNIEnv<'_>,
    buffer: &JByteBuffer<'local>,
) -> Result<&'local mut [u8]> {
    let ptr = env.get_direct_buffer_address(buffer)?;
    let capacity = env.get_direct_buffer_capacity(buffer)?;

    // SAFETY: `jni` rejects a null/non-direct buffer and a null address before
    // returning `ptr`; it also rejects JNI's `-1` capacity sentinel. The local
    // reference keeps the Java buffer alive for this native call, and callers do
    // not access the buffer concurrently while Rust owns this mutable slice.
    #[allow(unsafe_code)]
    Ok(unsafe { from_raw_parts_mut(ptr, capacity) })
}

/// Supplies the zero/default JNI return used while a Java exception is pending.
pub trait ThrowingHasDefault {
    /// Returns the JNI-compatible default value.
    fn default() -> Self;
}

impl ThrowingHasDefault for jboolean {
    fn default() -> Self {
        0 as jboolean
    }
}

impl ThrowingHasDefault for jobject {
    fn default() -> Self {
        0 as jobject
    }
}

impl ThrowingHasDefault for i32 {
    fn default() -> Self {
        0
    }
}

impl ThrowingHasDefault for i64 {
    fn default() -> Self {
        0
    }
}

impl ThrowingHasDefault for () {
    fn default() -> Self {}
}

/// Contains a Rust result or panic at the JNI boundary and throws `RuntimeException`.
///
/// When an error occurs, the return value is [`ThrowingHasDefault::default`] and
/// must only be interpreted while the Java exception remains pending. Error display
/// text becomes the Java exception message, so callers must never attach credentials,
/// URLs, response bodies, tokens, paths, or other sensitive input to boundary errors.
pub fn jni_throwing<F, R>(env: &mut JNIEnv<'_>, function: F) -> R
where
    F: FnOnce(&mut JNIEnv<'_>) -> Result<R>,
    R: ThrowingHasDefault,
{
    match catch_unwind(AssertUnwindSafe(|| function(env))) {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            let message = format!("{error}");
            env.throw_new("java/lang/RuntimeException", message).ok();
            R::default()
        }
        Err(_) => {
            env.throw_new(
                "java/lang/RuntimeException",
                "Rust panic contained at JNI boundary",
            )
            .ok();
            R::default()
        }
    }
}

/// Validates a positive byte length and UTF-8 direct-buffer prefix.
///
/// # Errors
///
/// Returns an error for non-positive or out-of-capacity lengths and invalid UTF-8.
pub fn input_body(buffer: &[u8], limit: jint) -> Result<&str> {
    codec::input_text(buffer, limit)
}

/// Serializes one value atomically into a caller-owned direct-buffer slice.
///
/// An undersized output remains unchanged.
///
/// # Errors
///
/// Returns an error when CBOR serialization fails, the encoded length does not fit
/// JNI's signed return type, or the destination is too small.
pub fn serialize_to_buffer<T: Serialize>(buffer: &mut [u8], value: &T) -> Result<i32> {
    codec::encode_into(buffer, value)
}

fn decode_request<T: DeserializeOwned>(buffer: &[u8], limit: jint) -> Result<T> {
    codec::decode(buffer, limit)
}

fn parse_text_in_place<F, R>(
    env: &mut JNIEnv<'_>,
    buffer: &JByteBuffer<'_>,
    limit: jint,
    parse: F,
) -> jint
where
    F: Fn(&str) -> R,
    R: Serialize,
{
    jni_throwing(env, |env| {
        let buffer = deref_mut_direct_bytebuffer(env, buffer)?;
        let value = {
            let body = codec::input_text(buffer, limit)?;
            parse(body)
        };
        codec::encode_into(buffer, &value)
    })
}
