//! Primitive raw-UTF16 comment preprocessing; Android retains Unicode digit facts.
use super::jni_throwing;
use anyhow::{anyhow, ensure};
use eh_domain::comment_display::{self, DisplayError, MAX_TEXT};
use jni::{
    JNIEnv,
    objects::{JByteArray, JCharArray, JClass},
    sys::jintArray,
};
use jni_fn::jni_fn;

fn observations(
    env: &JNIEnv<'_>,
    text: &JCharArray<'_>,
    digits: &JByteArray<'_>,
) -> anyhow::Result<(Vec<u16>, Vec<u8>)> {
    let size = usize::try_from(env.get_array_length(text)?)?;
    ensure!(
        size <= MAX_TEXT && usize::try_from(env.get_array_length(digits)?)? == size,
        "Invalid comment observations"
    );
    let mut units = vec![0; size];
    env.get_char_array_region(text, 0, &mut units)?;
    Ok((units, env.convert_byte_array(digits)?))
}

/// Return UTF-16 range pairs for the original comment-link grammar.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.CommentDisplayBridgeKt")]
pub fn nativeCommentLinksV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    text: JCharArray<'_>,
    digits: JByteArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let (text, digits) = observations(env, &text, &digits)?;
        let values =
            comment_display::links(&text, &digits).map_err(|error| anyhow!(error.message()))?;
        let output = env.new_int_array(i32::try_from(values.len())?)?;
        env.set_int_array_region(&output, 0, &values)?;
        Ok(output.into_raw())
    })
}

/// Return source/target dimensions, or null when the image grammar does not match.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.CommentDisplayBridgeKt")]
pub fn nativeCommentImageDimensionsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    text: JCharArray<'_>,
    digits: JByteArray<'_>,
) -> jintArray {
    jni_throwing(&mut env, |env| {
        let (text, digits) = observations(env, &text, &digits)?;
        let result = match comment_display::image_dimensions(&text, &digits) {
            Ok(result) => result,
            Err(DisplayError::Number) => {
                env.throw_new(
                    "java/lang/NumberFormatException",
                    DisplayError::Number.message(),
                )?;
                return Ok(std::ptr::null_mut());
            }
            Err(error) => return Err(anyhow!(error.message())),
        };
        let Some(result) = result else {
            return Ok(std::ptr::null_mut());
        };
        let output = env.new_int_array(4)?;
        env.set_int_array_region(&output, 0, &result)?;
        Ok(output.into_raw())
    })
}
