//! JVM direct-buffer adapters for the pure `ComicInfo` owner.

use eh_domain::comic_info::{
    ComicInfoDecodeRequest, ComicInfoEncodeRequest, GalleryComicInfoRequest,
    build_gallery_envelope, decode_xml_envelope, encode_xml_envelope,
};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

/// Projects gallery metadata into `ComicInfo` through caller-owned direct buffers.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ComicInfoBridgeKt")]
pub fn nativeBuildComicInfoV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryComicInfoRequest = decode_request(request_bytes, request_length)?;
        let response = build_gallery_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Encodes `ComicInfo` XML through caller-owned direct buffers.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ComicInfoBridgeKt")]
pub fn nativeEncodeComicInfoXmlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: ComicInfoEncodeRequest = decode_request(request_bytes, request_length)?;
        let response = encode_xml_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Decodes `ComicInfo` XML through caller-owned direct buffers.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ComicInfoBridgeKt")]
pub fn nativeDecodeComicInfoXmlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: ComicInfoDecodeRequest = decode_request(request_bytes, request_length)?;
        let response = decode_xml_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}
