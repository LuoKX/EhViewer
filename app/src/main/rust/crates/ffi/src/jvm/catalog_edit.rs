//! Bounded list/quick-search plans and primitive cross-store favorite intents.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{anyhow, ensure};
use eh_domain::{favorite_batch, list_order, quick_search};
use jni::{
    JNIEnv,
    objects::{JByteBuffer, JClass},
    sys::{jbyteArray, jint, jintArray},
};
use jni_fn::jni_fn;

/// Plans a complete quick-search check or creation phase.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.QuickSearchEditBridgeKt")]
pub fn nativeQuickSearchEditV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=quick_search::MAX_BYTES).contains(&size),
            "Invalid quick search request size"
        );
        let request: quick_search::Request =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)
                .map_err(|_| anyhow!("Invalid quick search CBOR"))?;
        let bytes = serde_cbor::to_vec(&quick_search::build_envelope(&request))?;
        ensure!(
            bytes.len() <= usize::try_from(quick_search::MAX_BYTES)?,
            "Quick search response exceeds limit"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}

/// Plans a complete list edit before Android mutates the observed list.
#[jni_fn("com.hippo.ehviewer.bridge.library.ListOrderBridgeKt")]
pub fn nativeListOrderV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=list_order::MAX_BYTES).contains(&size),
            "Invalid list order request size"
        );
        let request: list_order::Request =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)
                .map_err(|_| anyhow!("Invalid list order CBOR"))?;
        let bytes = serde_cbor::to_vec(&list_order::build_envelope(&request))?;
        ensure!(
            bytes.len() <= usize::try_from(list_order::MAX_BYTES)?,
            "List order response exceeds limit"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}

/// Plans favorite effects without receiving galleries, tokens or credentials.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.FavoriteBatchBridgeKt")]
pub fn nativeFavoriteBatchV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    operation: jint,
    left: jint,
    right: jint,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let plan = favorite_batch::plan(operation, left, right).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
