//! V1 portable reader policy transport; no Android image or exception references.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use eh_domain::reader_policy::{ReaderPolicyRequest, build_envelope, image_entry_size};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jint, jlong};
use jni_fn::jni_fn;
use serde::Deserialize;

/// V1 versioned complete policy request.
#[derive(Deserialize)]
struct WireRequest {
    version: u8,
    #[serde(flatten)]
    request: ReaderPolicyRequest,
}

/// Invokes one closed reader rule through V1 CBOR.
#[jni_fn("com.hippo.ehviewer.bridge.reader.ReaderPolicyBridgeKt")]
pub fn nativePlanReaderPolicyV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request: WireRequest = decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &build_envelope(request.version, &request.request),
        )
    })
}

/// Accounts one cached image using scalar transport to avoid allocating during cache insertion.
#[jni_fn("com.hippo.ehviewer.bridge.reader.ReaderPolicyBridgeKt")]
pub fn nativeReaderImageEntrySizeV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    version: jint,
    allocation_size: jlong,
    cache_size: jint,
) -> jint {
    jni_throwing(&mut env, |_| {
        anyhow::ensure!(version == 1, "Unsupported reader entry API version");
        Ok(image_entry_size(allocation_size, cache_size)?)
    })
}
