//! V1 Gallery language and eligibility transport.
use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};
use eh_domain::gallery_enrichment::{EnrichmentRequest, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

/// Plans one complete language/eligibility operation without Android-side business fallback.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryEnrichmentBridgeKt")]
pub fn nativePlanGalleryEnrichmentV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: EnrichmentRequest = decode_request(bytes, request_length)?;
        let envelope = build_envelope(&request);
        serialize_to_buffer(
            deref_mut_direct_bytebuffer(env, &response_buffer)?,
            &envelope,
        )
    })
}
