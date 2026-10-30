//! Primitive, bounded restart projection; no row identifiers or persistence access.
use super::jni_throwing;
use anyhow::ensure;
use eh_domain::download_startup::{MAX_STATES, normalize_states};
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray},
    sys::jintArray,
};
use jni_fn::jni_fn;

/// Returns normalized states in the exact input order.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadStartupBridgeKt")]
pub fn nativeNormalizeDownloadStatesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    states: JIntArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let count = usize::try_from(env.get_array_length(&states)?)?;
        ensure!(count <= MAX_STATES, "Download state snapshot exceeds limit");
        let mut input = vec![0; count];
        env.get_int_array_region(&states, 0, &mut input)?;
        let plan = normalize_states(&input).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
