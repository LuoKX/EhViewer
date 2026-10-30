//! Update-container FD adapter; descriptors stay caller-owned.
use super::{duplicate_file_descriptor, jni_throwing};
use anyhow::ensure;
use eh_archive::update_package::extract_apk;
use jni::{
    JNIEnv,
    objects::JClass,
    sys::{jint, jlong},
};
use jni_fn::jni_fn;
use std::io::{Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;

/// Extract one APK into a distinct private staging FD. Failed output is truncated.
#[jni_fn("com.hippo.ehviewer.bridge.updater.UpdatePackageBridgeKt")]
pub fn nativeExtractUpdateApkV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    source: jint,
    output: jint,
) -> jlong {
    jni_throwing(&mut env, |_| {
        let mut source = duplicate_file_descriptor(source, "Update input")?;
        let mut output = duplicate_file_descriptor(output, "Update output")?;
        let a = source.metadata()?;
        let b = output.metadata()?;
        ensure!(
            a.is_file() && b.is_file() && (a.dev(), a.ino()) != (b.dev(), b.ino()),
            "Invalid update descriptors"
        );
        output.set_len(0)?;
        output.seek(SeekFrom::Start(0))?;
        let result = (|| -> anyhow::Result<i64> {
            let size = extract_apk(&mut source, &mut output)?;
            output.sync_all()?;
            Ok(i64::try_from(size)?)
        })();
        if result.is_err() {
            let _ = output.set_len(0);
            let _ = output.seek(SeekFrom::Start(0));
        }
        result
    })
}
