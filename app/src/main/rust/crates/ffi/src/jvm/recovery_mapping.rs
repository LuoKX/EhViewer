//! Stateless bounded recovery-association transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::{anyhow, ensure};
use eh_domain::recovery_mapping::{MAX_REQUEST_BYTES, Request, build_envelope};
use jni::{
    JNIEnv,
    objects::{JByteBuffer, JClass},
    sys::jint,
};
use jni_fn::jni_fn;

/// Associate one immutable scan/download snapshot without file or database effects.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryMappingBridgeKt")]
pub fn nativePlanRecoveryMappingsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=MAX_REQUEST_BYTES).contains(&size),
            "Invalid recovery mapping request size"
        );
        let request: Request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid recovery mapping CBOR"))?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &build_envelope(&request),
        )
    })
}
