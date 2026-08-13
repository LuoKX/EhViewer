//! JNI transport for the Gallery list and comment filter planner.

use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use eh_domain::contracts::ApiEnvelope;
use eh_domain::gallery_filter::{
    GALLERY_FILTER_API_VERSION, GALLERY_FILTER_MODULE, GalleryFilterPlan, GalleryFilterRequest,
    plan_gallery_filter,
};

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

fn build_envelope(
    request: &GalleryFilterRequest,
) -> anyhow::Result<ApiEnvelope<GalleryFilterPlan>> {
    let plan = plan_gallery_filter(request)?;
    Ok(ApiEnvelope::ok(
        GALLERY_FILTER_API_VERSION,
        GALLERY_FILTER_MODULE,
        plan,
    ))
}

/// Plans one complete pre-enrichment, post-enrichment, or comment filtering batch.
///
/// Android supplies one versioned CBOR request and any aligned ICU comment match
/// facts. Rust returns one atomic keep/drop plan and retains no JVM references.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryFilterBridgeKt")]
pub fn nativePlanGalleryFilterV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryFilterRequest = decode_request(request_bytes, request_length)?;
        let response = build_envelope(&request)?;
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

#[cfg(test)]
mod tests {
    use eh_domain::gallery_filter::{
        GALLERY_FILTER_API_VERSION, GalleryFilterOperation, GalleryFilterRequest,
    };

    use super::*;

    #[test]
    fn builds_the_exact_v1_envelope() {
        let request = empty_request(GALLERY_FILTER_API_VERSION);

        let envelope = build_envelope(&request).expect("supported request should plan");

        assert_eq!(envelope.version, GALLERY_FILTER_API_VERSION);
        assert_eq!(envelope.module, GALLERY_FILTER_MODULE);
        assert!(envelope.error.is_none());
        let plan = envelope.payload.expect("success payload");
        assert_eq!(plan.operation, GalleryFilterOperation::PreEnrichment);
        assert!(plan.keep.is_empty());
    }

    #[test]
    fn rejects_an_unsupported_version_before_serializing_a_response() {
        let error = build_envelope(&empty_request(2)).expect_err("version drift must fail");

        assert_eq!(error.to_string(), "Unsupported Gallery filter API version");
    }

    fn empty_request(version: u8) -> GalleryFilterRequest {
        GalleryFilterRequest {
            version,
            operation: GalleryFilterOperation::PreEnrichment,
            filters: vec![],
            galleries: vec![],
            comments: vec![],
            score_threshold: None,
            comment_content_matches: None,
        }
    }
}
