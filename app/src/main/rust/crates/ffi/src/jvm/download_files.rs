//! Small primitive JNI contracts for names/hash grammar and batched cleanup.
use super::jni_throwing;
use anyhow::{anyhow, ensure};
use eh_domain::download_files::{
    MAX_EXTENSION, MAX_FILES, MAX_URL, expected_image_hash, page_filename, post_archive_pages,
};
use jni::{
    JNIEnv,
    objects::{JBooleanArray, JCharArray, JClass},
    sys::{jcharArray, jint, jintArray, jstring},
};
use jni_fn::jni_fn;

fn chars(env: &JNIEnv<'_>, array: &JCharArray<'_>, limit: usize) -> anyhow::Result<Vec<u16>> {
    let size = usize::try_from(env.get_array_length(array)?)?;
    ensure!(size <= limit, "Download text exceeds limit");
    let mut result = vec![0; size];
    env.get_char_array_region(array, 0, &mut result)?;
    Ok(result)
}

/// Format one page name without CBOR or UTF-8 conversion of Java strings.
#[jni_fn("com.hippo.ehviewer.bridge.files.DownloadFilesBridgeKt")]
pub fn nativePageFilenameV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    index: jint,
    zero: jint,
    extension: JCharArray<'_>,
) -> jcharArray {
    jni_throwing(&mut env, |env| {
        let zero = u16::try_from(zero).map_err(|_| anyhow!("Invalid locale digit"))?;
        let name = page_filename(index, zero, &chars(env, &extension, MAX_EXTENSION)?)
            .map_err(anyhow::Error::msg)?;
        let result = env.new_char_array(i32::try_from(name.len())?)?;
        env.set_char_array_region(&result, 0, &name)?;
        Ok(result.into_raw())
    })
}

/// Read the fixed download hash grammar; result is ASCII or null.
#[jni_fn("com.hippo.ehviewer.bridge.files.DownloadFilesBridgeKt")]
pub fn nativeExpectedImageHashV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    url: JCharArray<'_>,
) -> jstring {
    jni_throwing(&mut env, |env| {
        let hash = expected_image_hash(&chars(env, &url, MAX_URL)?).map_err(anyhow::Error::msg)?;
        hash.map_or(Ok(std::ptr::null_mut()), |hash| {
            env.new_string(hash)
                .map(jni::objects::JString::into_raw)
                .map_err(Into::into)
        })
    })
}

/// Return original provider slots to delete, with a hard observation limit.
#[jni_fn("com.hippo.ehviewer.bridge.files.DownloadFilesBridgeKt")]
pub fn nativePostArchivePagesV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    matches: JBooleanArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let count = usize::try_from(env.get_array_length(&matches)?)?;
        ensure!(count <= MAX_FILES, "Download cleanup exceeds limit");
        let mut values = vec![0; count];
        env.get_boolean_array_region(&matches, 0, &mut values)?;
        let plan = post_archive_pages(
            &values
                .into_iter()
                .map(|value| value != 0)
                .collect::<Vec<_>>(),
        )
        .map_err(anyhow::Error::msg)?;
        let result = env.new_int_array(i32::try_from(plan.len())?)?;
        env.set_int_array_region(&result, 0, &plan)?;
        Ok(result.into_raw())
    })
}
