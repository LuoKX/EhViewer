//! Versioned file rules transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Executes one complete Rust owner through bounded V1 CBOR.
#[jni_fn("com.hippo.ehviewer.bridge.files.FileRulesBridgeKt")]
pub fn nativePlanFileRulesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request: eh_domain::file_rules::FileRulesRequest =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &eh_domain::file_rules::build_envelope(&request),
        )
    })
}
