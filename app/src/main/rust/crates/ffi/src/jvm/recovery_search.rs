//! Versioned recovery search transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Invokes the complete Rust owner through V1 CBOR.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoverySearchBridgeKt")]
pub fn nativePlanRecoverySearchV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request: eh_domain::recovery_search::RecoverySearchRequest =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &eh_domain::recovery_search::build_envelope(&request),
        )
    })
}
