//! Bounded, redacted V1 preview pagination and sparse token delta bridge.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use eh_domain::spider_info::preview_tokens::{
    MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, PreviewTokenRequest, build_envelope,
};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jbyteArray, jint};
use jni_fn::jni_fn;

/// Plans one lookup or completed preview response; never receives the current token map.
#[jni_fn("com.hippo.ehviewer.bridge.reader.PreviewTokenBridgeKt")]
pub fn nativePlanPreviewTokensV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        anyhow::ensure!(
            size > 0 && usize::try_from(size)? <= MAX_REQUEST_BYTES,
            "Invalid preview token request size"
        );
        let request: PreviewTokenRequest =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)
                .map_err(|_| anyhow::anyhow!("Invalid preview token request"))?;
        let bytes = serde_cbor::to_vec(&build_envelope(&request))?;
        anyhow::ensure!(
            bytes.len() <= MAX_RESPONSE_BYTES,
            "Invalid preview token response size"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}
