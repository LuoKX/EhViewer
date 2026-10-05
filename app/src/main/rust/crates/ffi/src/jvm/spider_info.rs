//! V1 full `SpiderInfo` codec, retaining the original unversioned storage format.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use eh_domain::spider_info::{SpiderInfoRequest, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jbyteArray, jint};
use jni_fn::jni_fn;
use serde::Deserialize;

#[derive(Deserialize)]
struct WireRequest {
    version: u8,
    #[serde(flatten)]
    request: SpiderInfoRequest,
}

/// Invokes one complete Rust codec operation with a capped exact-sized response.
#[jni_fn("com.hippo.ehviewer.bridge.reader.SpiderInfoBridgeKt")]
pub fn nativeCodecSpiderInfoV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        anyhow::ensure!(
            size > 0 && size <= 2 * 1024 * 1024,
            "SpiderInfo request exceeds resource budget"
        );
        let request: WireRequest = decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        let bytes = serde_cbor::to_vec(&build_envelope(request.version, &request.request))?;
        anyhow::ensure!(
            bytes.len() <= 2 * 1024 * 1024,
            "SpiderInfo response exceeds resource budget"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}
