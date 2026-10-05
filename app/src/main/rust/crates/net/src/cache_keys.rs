//! Compatible thumbnail/image cache keys and timed-preview normalization.
use eh_domain::contracts::{ApiEnvelope, RustError};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const THUMB_E: &str = "https://ehgt.org/";
const THUMB_EX: &str = "https://s.exhentai.org/";

/// Closed V1 cache-key operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheKeyKind {
    /// Image key from gallery and page index.
    Image,
    /// Strip each historical thumbnail prefix in order.
    Thumb,
    /// Normalize a timed v2 preview path.
    Preview,
    /// Restore a thumbnail URL for the selected site.
    Url,
}

/// Versioned inputs; credentials never enter this boundary.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyRequest {
    /// Protocol version.
    pub version: u8,
    /// Complete requested operation.
    pub kind: CacheKeyKind,
    /// Raw key or URL for non-image operations.
    pub input: Option<String>,
    /// Gallery identity for image operations.
    pub gid: Option<i64>,
    /// Zero-based page index for image operations.
    pub index: Option<i32>,
    /// Selected E/EX site for URL reconstruction.
    pub ex_hentai: Option<bool>,
}

/// Complete key/URL result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CacheKeyPlan {
    /// Requested operation echoed for validation.
    pub kind: CacheKeyKind,
    /// Exact compatible key or URL.
    pub value: String,
}

/// Plans one complete cache-key transformation.
#[must_use]
pub fn build_envelope(request: &CacheKeyRequest) -> ApiEnvelope<CacheKeyPlan> {
    let result = plan(request);
    match result {
        Ok(value) => ApiEnvelope::ok(
            1,
            "cacheKey",
            CacheKeyPlan {
                kind: request.kind,
                value,
            },
        ),
        Err(error) => ApiEnvelope::err(1, "cacheKey", error),
    }
}

fn plan(request: &CacheKeyRequest) -> Result<String, RustError> {
    if request.version != 1 {
        return Err(invalid());
    }
    if request.kind == CacheKeyKind::Image {
        return Ok(format!(
            "image:{}:{}",
            request.gid.ok_or_else(invalid)?,
            request.index.ok_or_else(invalid)?
        ));
    }
    let input = request
        .input
        .as_deref()
        .filter(|input| input.len() <= 16 * 1024)
        .ok_or_else(invalid)?;
    Ok(match request.kind {
        CacheKeyKind::Image => unreachable!("image operation handled before text input"),
        CacheKeyKind::Thumb => {
            let key = input.strip_prefix(THUMB_E).unwrap_or(input);
            let key = key.strip_prefix("https://s.exhentai.org/t/").unwrap_or(key);
            key.strip_prefix(THUMB_EX).unwrap_or(key).to_owned()
        }
        CacheKeyKind::Preview => {
            static PATTERN: OnceLock<Regex> = OnceLock::new();
            let captures = PATTERN
                .get_or_init(|| {
                    Regex::new(r"/c([m12]/)[^/]+/(\d+-\d+)").expect("fixed preview pattern")
                })
                .captures(input);
            captures.map_or_else(
                || format!("${input}"),
                |captures| format!("${}{}", &captures[1], &captures[2]),
            )
        }
        CacheKeyKind::Url => {
            let ex_hentai = request.ex_hentai.ok_or_else(invalid)?;
            if input.starts_with("https:") {
                input.to_owned()
            } else {
                let prefix = if ex_hentai {
                    if input.ends_with("webp") {
                        THUMB_EX
                    } else {
                        "https://s.exhentai.org/t/"
                    }
                } else {
                    THUMB_E
                };
                format!("{prefix}{input}")
            }
        }
    })
}
fn invalid() -> RustError {
    RustError::Error("Invalid cache key input".into())
}

#[cfg(test)]
mod tests;
