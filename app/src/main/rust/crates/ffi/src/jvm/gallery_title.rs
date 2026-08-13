//! JNI transport for the similar-gallery title planner.

use anyhow::ensure;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use eh_domain::contracts::ApiEnvelope;
use eh_domain::gallery_rules::{
    GALLERY_TITLE_API_VERSION, GALLERY_TITLE_MODULE, GalleryTitlePlan, GalleryTitleRequest,
    plan_gallery_title,
};

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

fn build_envelope(request: &GalleryTitleRequest) -> anyhow::Result<ApiEnvelope<GalleryTitlePlan>> {
    ensure!(
        request.version == GALLERY_TITLE_API_VERSION,
        "Unsupported Gallery title API version"
    );
    Ok(ApiEnvelope::ok(
        GALLERY_TITLE_API_VERSION,
        GALLERY_TITLE_MODULE,
        plan_gallery_title(request),
    ))
}

/// Plans one complete similar-gallery title cleanup.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryTitleBridgeKt")]
pub fn nativePlanGalleryTitleV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryTitleRequest = decode_request(request_bytes, request_length)?;
        let response = build_envelope(&request)?;
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_exact_v1_envelope() {
        let request = GalleryTitleRequest {
            version: GALLERY_TITLE_API_VERSION,
            input: Some("[Group] Title CHx 1-2".to_owned()),
        };

        let envelope = build_envelope(&request).expect("supported request should plan");

        assert_eq!(envelope.version, GALLERY_TITLE_API_VERSION);
        assert_eq!(envelope.module, GALLERY_TITLE_MODULE);
        assert!(envelope.error.is_none());
        assert_eq!(
            envelope.payload.expect("success payload").title.as_deref(),
            Some("Title")
        );
    }

    #[test]
    fn rejects_an_unsupported_version_before_serializing_a_response() {
        let request = GalleryTitleRequest {
            version: 2,
            input: None,
        };

        let error = build_envelope(&request).expect_err("version drift must fail");

        assert_eq!(error.to_string(), "Unsupported Gallery title API version");
    }
}
