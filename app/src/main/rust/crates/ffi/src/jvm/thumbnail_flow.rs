//! Thumbnail phase intentions without Coil objects or filesystem handles.
use super::jni_throwing;
use anyhow::ensure;
use eh_domain::thumbnail_flow;
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray},
    sys::{jint, jintArray},
};
use jni_fn::jni_fn;

/// Execute one bounded thumbnail phase.
#[jni_fn("com.hippo.ehviewer.bridge.files.ThumbnailFlowBridgeKt")]
pub fn nativeThumbnailFlowV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    operation: jint,
    facts: JIntArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let size = usize::try_from(env.get_array_length(&facts)?)?;
        ensure!(
            size <= thumbnail_flow::MAX_FACTS,
            "Thumbnail request exceeds limit"
        );
        let mut values = vec![0; size];
        env.get_int_array_region(&facts, 0, &mut values)?;
        let plan = thumbnail_flow::plan(operation, &values).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
