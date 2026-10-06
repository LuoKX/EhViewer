//! Single bounded V1 backup import transport with payload-free diagnostics.
use anyhow::ensure;
use eh_domain::backup_import::{
    BackupImportRequest, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, build_envelope,
};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use super::{
    deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing, serialize_to_buffer,
};

/// Plans complete snapshot before Android Room/list effects.
#[jni_fn("com.hippo.ehviewer.bridge.download.BackupImportBridgeKt")]
pub fn nativePlanBackupImportV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request: JByteBuffer<'_>,
    size: jint,
    response: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let input = deref_direct_bytebuffer(env, &request)?;
        let length = usize::try_from(size).unwrap_or_default();
        ensure!(
            length > 0 && length <= MAX_REQUEST_BYTES && length <= input.len(),
            "Invalid backup import request size"
        );
        // Do not expose serde errors: unknown field/type details may contain backup names.
        let request: BackupImportRequest = serde_cbor::from_slice(&input[..length])
            .map_err(|_| anyhow::anyhow!("Invalid backup import CBOR"))?;
        let output = deref_mut_direct_bytebuffer(env, &response)?;
        let capacity = output.len().min(MAX_RESPONSE_BYTES);
        serialize_to_buffer(&mut output[..capacity], &build_envelope(&request))
    })
}
