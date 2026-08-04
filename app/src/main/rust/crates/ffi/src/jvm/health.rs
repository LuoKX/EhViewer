//! JVM adapter for the stable Rust Core health operation.

use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use crate::health::{RustCoreHealthRequest, build_envelope};

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

/// Runs one V1 Rust Core health request through caller-owned direct buffers.
///
/// The input is borrowed immutably only for synchronous CBOR decoding. The response
/// is serialized into temporary storage and then copied atomically into the mutable
/// output buffer; Rust retains neither buffer nor the scalar request fields.
///
/// A null/non-direct buffer, invalid length or CBOR, insufficient output capacity,
/// or contained panic becomes a pending Java `RuntimeException` and returns zero.
/// Unsupported request versions remain typed V1 error envelopes.
#[jni_fn("com.hippo.ehviewer.bridge.RustBridgeKt")]
pub fn nativeRustCoreHealthV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: RustCoreHealthRequest = decode_request(request_bytes, request_length)?;
        let response = build_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}
