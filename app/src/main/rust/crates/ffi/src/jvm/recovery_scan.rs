//! Compact recovery scan transport: one CBOR open, UTF-16 ID bytes and scalar steps.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{anyhow, ensure};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jint, jlong};
use jni_fn::jni_fn;

/// Open one bounded, owned scan. Errors never contain provider identities.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryScanBridgeKt")]
pub fn nativeOpenRecoveryScanV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jlong {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=262_144).contains(&size),
            "Invalid recovery scan request size"
        );
        let options = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid recovery scan CBOR request"))?;
        crate::recovery_scan::create(options).map_err(anyhow::Error::msg)
    })
}

/// Primitive operations: poll=1, gate=2, accept=3, finish=4, limit=5, close=6, stop=7.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryScanBridgeKt")]
pub fn nativeStepRecoveryScanV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    handle: jlong,
    operation: jint,
    argument: jlong,
) -> jlong {
    jni_throwing(&mut env, |_| {
        crate::recovery_scan::step(handle, operation, argument).map_err(anyhow::Error::msg)
    })
}

/// Observe one provider identity as raw UTF-16LE, without CBOR boxing per cursor row.
#[jni_fn("com.hippo.ehviewer.bridge.download.RecoveryScanBridgeKt")]
pub fn nativeObserveRecoveryDocumentV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    handle: jlong,
    identity: JByteBuffer<'_>,
    size: jint,
) -> jlong {
    jni_throwing(&mut env, |env| {
        ensure!(
            (0..=262_144).contains(&size),
            "Invalid recovery document identity size"
        );
        let bytes = deref_direct_bytebuffer(env, &identity)?;
        let id = bytes
            .get(..usize::try_from(size)?)
            .ok_or_else(|| anyhow!("Recovery identity exceeds buffer capacity"))?;
        crate::recovery_scan::observe(handle, id)
            .map(i64::from)
            .map_err(anyhow::Error::msg)
    })
}
