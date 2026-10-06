//! Fixed-size, scalar-only `ComicInfo` policy transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use anyhow::ensure;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plan one operation phase; reject oversized requests before CBOR decode.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ComicInfoPolicyBridgeKt")]
pub fn nativePlanComicInfoPolicyV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        ensure!(
            (1..=4096).contains(&size),
            "Invalid ComicInfo policy request size"
        );
        let request: eh_domain::comic_info_policy::ComicInfoPolicyRequest =
            decode_request(deref_direct_bytebuffer(env, &request)?, size)?;
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response)?,
            &eh_domain::comic_info_policy::build_envelope(&request),
        )
    })
}
