//! JVM adapters for the Rust-owned animated WebP decoder registry.

use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jlong;
use jni_fn::jni_fn;

use super::{deref_direct_bytebuffer, jni_throwing};
use crate::webp::{create_decoder, destroy_decoder, get_image_info, pack_image_info};

/// Copies a direct Java buffer into a Rust-owned animated WebP decoder.
///
/// The returned positive integer is an opaque registry handle, never a native
/// pointer. Invalid or empty input produces a contained Java `RuntimeException`.
#[jni_fn("com.hippo.ehviewer.coil.AnimatedWebPDrawableKt")]
pub fn nativeCreateDecoder(mut env: JNIEnv<'_>, _: JClass<'_>, buffer: JByteBuffer<'_>) -> jlong {
    jni_throwing(&mut env, |env| {
        let buffer = deref_direct_bytebuffer(env, &buffer)?;
        create_decoder(buffer)
    })
}

/// Returns packed width, height, and loop-count metadata for an opaque decoder handle.
///
/// Width and height occupy 24 bits each and loop count occupies 16 bits. Unknown
/// handles produce a contained Java `RuntimeException`.
#[jni_fn("com.hippo.ehviewer.coil.AnimatedWebPDrawableKt")]
pub fn nativeGetImageInfo(mut env: JNIEnv<'_>, _: JClass<'_>, decoder: jlong) -> jlong {
    jni_throwing(&mut env, |_| {
        let info = get_image_info(decoder)?;
        Ok(pack_image_info(info).cast_signed())
    })
}

/// Idempotently removes and releases an opaque animated WebP decoder handle.
///
/// Zero, negative, unknown, and already-destroyed handles are safe no-ops.
#[jni_fn("com.hippo.ehviewer.coil.AnimatedWebPDrawableKt")]
pub fn nativeDestroyDecoder(mut env: JNIEnv<'_>, _: JClass<'_>, decoder: jlong) {
    jni_throwing(&mut env, |_| {
        destroy_decoder(decoder)?;
        Ok(())
    });
}
