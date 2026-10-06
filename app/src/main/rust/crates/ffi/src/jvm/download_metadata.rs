//! Bounded, payload-redacted download metadata batch transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::{anyhow, ensure};
use eh_domain::download_metadata::{MAX_WIRE_BYTES, Request, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plan one bounded observation or patch phase, without Android entities or I/O.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadMetadataBridgeKt")]
pub fn nativePlanDownloadMetadataV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            size > 0 && usize::try_from(size)? <= MAX_WIRE_BYTES,
            "Invalid download metadata request size"
        );
        let request: Request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid download metadata CBOR"))?;
        let output = deref_mut_direct_bytebuffer(env, &response)?;
        let capacity = output.len().min(MAX_WIRE_BYTES);
        serialize_to_buffer(&mut output[..capacity], &build_envelope(&request))
    })
}
