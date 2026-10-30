//! Primitive discrete-event reader bridge; no Compose state or handles cross JNI.
use super::jni_throwing;
use anyhow::ensure;
use eh_domain::reader_navigation;
use jni::{
    JNIEnv,
    objects::{JClass, JIntArray},
    sys::{jint, jintArray},
};
use jni_fn::jni_fn;

/// Execute one bounded reader phase using exact Float bits.
#[jni_fn("com.hippo.ehviewer.bridge.reader.ReaderNavigationBridgeKt")]
pub fn nativeReaderNavigationV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    operation: jint,
    facts: JIntArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let size = usize::try_from(env.get_array_length(&facts)?)?;
        ensure!(
            size <= reader_navigation::MAX_FACTS,
            "Reader navigation request exceeds limit"
        );
        let mut values = vec![0; size];
        env.get_int_array_region(&facts, 0, &mut values)?;
        let plan = reader_navigation::plan(operation, &values).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
