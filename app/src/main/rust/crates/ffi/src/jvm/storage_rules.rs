//! Bounded stateless storage-plan CBOR transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::{anyhow, ensure};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plan classification, sorting or retention without performing platform effects.
#[jni_fn("com.hippo.ehviewer.bridge.files.StorageRulesBridgeKt")]
pub fn nativePlanStorageRulesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=33_554_432).contains(&size),
            "Invalid storage rule request size"
        );
        let request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid storage rule CBOR request"))?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &eh_domain::storage_rules::build_envelope(&request),
        )
    })
}
