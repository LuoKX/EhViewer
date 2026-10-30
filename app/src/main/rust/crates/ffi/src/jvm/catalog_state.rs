//! Catalog transitions and numeric controls, without UI or URL dependencies.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{anyhow, ensure};
use eh_domain::catalog_state::{MAX_REQUEST_BYTES, Request, build_envelope, numbers};
use jni::{
    JNIEnv,
    objects::{JByteBuffer, JClass},
    sys::{jbyteArray, jint, jintArray},
};
use jni_fn::jni_fn;

/// Plans one bounded opaque cursor transition.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.CatalogStateBridgeKt")]
pub fn nativeCatalogNavigationV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=MAX_REQUEST_BYTES).contains(&size),
            "Invalid catalog request size"
        );
        let request: Request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid catalog CBOR"))?;
        let response = serde_cbor::to_vec(&build_envelope(&request))?;
        Ok(env.byte_array_from_slice(&response)?.into_raw())
    })
}

/// Resolves one constant-size numeric control operation.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.CatalogStateBridgeKt")]
pub fn nativeCatalogNumbersV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    operation: jint,
    left: jint,
    right: jint,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let plan = numbers(operation, left, right).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
