//! Bounded, versioned UTF-16 text transport. No text logging or persistence.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::ensure;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Execute one complete Rust text operation; limit serialized input before decode.
#[jni_fn("com.hippo.ehviewer.bridge.text.BBCodeBridgeKt")]
pub fn nativePlanBBCodeV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=8 * 1024 * 1024).contains(&size),
            "Invalid BBCode request size"
        );
        let request: eh_domain::bbcode::BbCodeRequest =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &eh_domain::bbcode::build_envelope(&request),
        )
    })
}
