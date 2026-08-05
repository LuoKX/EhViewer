//! JNI transport for the coarse Gallery network-policy planner.

use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use eh_net::gallery::network_policy::{GalleryNetworkPolicyRequest, build_envelope};

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

/// Plans one complete Gallery Metadata or Archive networking operation.
///
/// Android supplies one versioned CBOR request before HTTP begins. Rust returns
/// one atomic policy envelope; Android retains request execution, parsing,
/// concurrency, delays, and coroutine cancellation.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryNetworkPolicyBridgeKt")]
pub fn nativePlanGalleryNetworkPolicyV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryNetworkPolicyRequest = decode_request(request_bytes, request_length)?;
        let response = build_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}
