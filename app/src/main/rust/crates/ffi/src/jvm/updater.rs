//! One bounded CBOR update protocol/policy operation; remote payloads never enter diagnostics.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::ensure;
use eh_domain::contracts::ApiEnvelope;
use eh_net::updater::{MAX_REQUEST, UpdatePlan, UpdateRequest};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plan one response or helper batch; buffers remain JVM-owned and output is atomic.
#[jni_fn("com.hippo.ehviewer.bridge.updater.UpdaterBridgeKt")]
pub fn nativePlanUpdateV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            size > 0 && usize::try_from(size)? <= MAX_REQUEST,
            "Invalid update request size"
        );
        let bytes = deref_direct_bytebuffer(env, &request)?;
        ensure!(
            usize::try_from(size)? <= bytes.len(),
            "Invalid update request size"
        );
        let envelope: ApiEnvelope<UpdatePlan> = match decode_request::<UpdateRequest>(bytes, size) {
            Ok(request) => eh_net::updater::build_envelope(&request),
            Err(_) => ApiEnvelope::err(
                1,
                "updater",
                eh_domain::update_policy::error(eh_domain::update_policy::INVALID_INPUT),
            ),
        };
        serialize_to_buffer(deref_mut_direct_bytebuffer(env, &response)?, &envelope)
    })
}
