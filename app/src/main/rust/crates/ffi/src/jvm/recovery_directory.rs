//! Bounded, stateless directory/promotion CBOR transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::{anyhow, ensure};
use eh_domain::recovery_directory::{MAX_REQUEST_BYTES, Request, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plan one entire observation or promotion batch without platform effects.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryDirectoryBridgeKt")]
pub fn nativePlanRecoveryDirectoryV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=MAX_REQUEST_BYTES).contains(&size),
            "Invalid recovery directory request size"
        );
        let request: Request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid recovery directory CBOR request"))?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &build_envelope(&request),
        )
    })
}
