//! V1 transport for pure EH recovery decisions.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use eh_domain::recovery_eh_lookup::{LookupRequest, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plans one complete lookup, selection, apply or failure decision with no platform effects.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadRecoveryEhLookupBridgeKt")]
pub fn nativePlanRecoveryEhLookupV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request: LookupRequest = decode_request(
            deref_direct_bytebuffer(env, &request_buffer)?,
            request_length,
        )?;
        let response = build_envelope(&request);
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response_buffer)?,
            &response,
        )
    })
}
