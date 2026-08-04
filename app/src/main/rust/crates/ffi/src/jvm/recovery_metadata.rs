//! Recovery-only extraction of non-displayable SpiderInfo evidence.

use std::collections::BTreeMap;

use anyhow::{Context as _, ensure};
use eh_domain::recovery::MAX_SPIDER_INFO_BYTES;
use jni::JNIEnv;
use jni::objects::{JByteArray, JClass};
use jni::sys::jstring;
use jni_fn::jni_fn;
use serde::Deserialize;

use super::jni_throwing;

#[derive(Deserialize)]
struct SpiderInfoPageTokenWire {
    #[serde(default, rename = "pTokenMap")]
    page_tokens: BTreeMap<i32, String>,
}

/// Returns `pageIndex:pageToken` for the first valid page token, or null when absent.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadRecoveryBridgeKt")]
pub fn nativeExtractDownloadRecoveryPageTokenV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    spider_info: JByteArray<'_>,
) -> jstring {
    jni_throwing(&mut env, |env| {
        let bytes = env.convert_byte_array(&spider_info)?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= MAX_SPIDER_INFO_BYTES,
            "Invalid recovery SpiderInfo length"
        );
        let wire: SpiderInfoPageTokenWire =
            serde_cbor::from_slice(&bytes).context("Invalid recovery SpiderInfo CBOR")?;
        let evidence = wire
            .page_tokens
            .into_iter()
            .filter(|(page, token)| *page >= 0 && valid_page_token(token))
            .min_by_key(|(page, _)| *page);
        match evidence {
            Some((page, token)) => Ok(env.new_string(format!("{page}:{token}"))?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

fn valid_page_token(token: &str) -> bool {
    token.len() == 10 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}
