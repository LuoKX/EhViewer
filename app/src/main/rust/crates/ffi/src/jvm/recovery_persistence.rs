//! Bounded recovery decisions. Database objects and credentials remain in Android.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{anyhow, ensure};
use eh_domain::recovery_persistence;
use jni::{
    JNIEnv,
    objects::{JByteBuffer, JClass},
    sys::{jbyteArray, jint},
};
use jni_fn::jni_fn;

/// Plans one recovery phase, failing before the caller applies its effects.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryPersistenceBridgeKt")]
pub fn nativeRecoveryPersistenceV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=recovery_persistence::MAX_BYTES).contains(&size),
            "Invalid recovery persistence request size"
        );
        let request: recovery_persistence::Request =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)
                .map_err(|_| anyhow!("Invalid recovery persistence CBOR"))?;
        let bytes = serde_cbor::to_vec(&recovery_persistence::build_envelope(&request))?;
        ensure!(
            bytes.len() <= usize::try_from(recovery_persistence::MAX_BYTES)?,
            "Recovery persistence response exceeds limit"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}
