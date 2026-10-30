//! Versioned label plans and primitive action decisions.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{anyhow, ensure};
use eh_domain::{
    entry_actions,
    label_rules::{MAX_BYTES, Request, build_envelope},
};
use jni::{
    JNIEnv,
    objects::{JBooleanArray, JByteBuffer, JClass},
    sys::{jbyteArray, jint, jintArray},
};
use jni_fn::jni_fn;

/// Plans one label phase using an immutable snapshot.
#[jni_fn("com.hippo.ehviewer.bridge.download.LabelRulesBridgeKt")]
pub fn nativeLabelRulesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=MAX_BYTES).contains(&size),
            "Invalid label rules request size"
        );
        let request: Request = decode_request(deref_direct_bytebuffer(env, &request)?, size)
            .map_err(|_| anyhow!("Invalid label rules CBOR"))?;
        let response = serde_cbor::to_vec(&build_envelope(&request))?;
        ensure!(
            response.len() <= usize::try_from(MAX_BYTES)?,
            "Label rules response exceeds limit"
        );
        Ok(env.byte_array_from_slice(&response)?.into_raw())
    })
}

/// Resolves entry choices without receiving gallery identities or credentials.
#[jni_fn("com.hippo.ehviewer.bridge.download.EntryActionsBridgeKt")]
pub fn nativeEntryActionV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    operation: jint,
    left: jint,
    right: jint,
    facts: JBooleanArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let count = usize::try_from(env.get_array_length(&facts)?)?;
        ensure!(
            count <= entry_actions::MAX_FACTS,
            "Entry observations exceed limit"
        );
        let mut input = vec![0; count];
        env.get_boolean_array_region(&facts, 0, &mut input)?;
        let facts: Vec<bool> = input.into_iter().map(|v| v != 0).collect();
        let plan =
            entry_actions::plan(operation, left, right, &facts).map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
