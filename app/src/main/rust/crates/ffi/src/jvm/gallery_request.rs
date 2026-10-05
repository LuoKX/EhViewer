//! V1 complete Gallery request planning with exact-sized output.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::ensure;
use eh_net::gallery::request::{GalleryRequest, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jbyteArray, jint};
use jni_fn::jni_fn;

/// Returns one bounded request plan; API credentials never enter this adapter.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryRequestBridgeKt")]
pub fn nativePlanGalleryRequestV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            request_length <= 2 * 1024 * 1024,
            "Gallery request exceeds input budget"
        );
        let request: GalleryRequest = decode_request(
            deref_direct_bytebuffer(env, &request_buffer)?,
            request_length,
        )?;
        let bytes = serde_cbor::to_vec(&build_envelope(&request))?;
        ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "Gallery request exceeds output budget"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}
