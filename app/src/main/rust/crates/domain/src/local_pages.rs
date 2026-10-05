//! Shared numbered-page naming and stable local reader selection.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Supported local image extensions, shared with recovery.
pub const IMAGE_EXTENSIONS: [&str; 11] = [
    "jpeg", "jpg", "png", "gif", "webp", "bmp", "ico", "wbmp", "heic", "heif", "avif",
];

/// Parses the shared eight-ASCII-digit basename policy.
///
/// Reader accepts uppercase extensions and zero ordinals for compatibility;
/// recovery still requires lowercase extensions and positive one-based ordinals.
#[must_use]
pub fn page_number(name: &str, ignore_case: bool, allow_zero: bool) -> Option<u32> {
    let (stem, extension) = name.rsplit_once('.')?;
    if stem.len() != 8
        || !stem.bytes().all(|byte| byte.is_ascii_digit())
        || !IMAGE_EXTENSIONS.iter().any(|value| {
            if ignore_case {
                extension.eq_ignore_ascii_case(value)
            } else {
                extension == *value
            }
        })
    {
        return None;
    }
    stem.parse::<u32>()
        .ok()
        .filter(|page| allow_zero || *page > 0)
}

/// Bounded V1 directory observation. Paths and file access remain Android-owned.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPagesRequest {
    /// Contract version.
    pub version: u8,
    /// Unmodified child basenames in provider enumeration order.
    pub names: Vec<String>,
}

/// One validated index into the caller's directory snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPage {
    /// Index into the original name/path observation list.
    pub source_index: u32,
    /// Original extension case, retained for the image source adapter.
    pub extension: String,
}

/// Selects and stably sorts matching basenames without moving filesystem objects.
#[must_use]
#[allow(
    clippy::missing_panics_doc,
    reason = "input bounds and selection guarantee conversions and extensions"
)]
pub fn build_envelope(request: &LocalPagesRequest) -> ApiEnvelope<Vec<LocalPage>> {
    if request.version != 1
        || request.names.len() > 100_000
        || request.names.iter().any(|name| name.len() > 4_096)
    {
        return ApiEnvelope::err(
            1,
            "localPages",
            RustError::Error("Invalid local page input".into()),
        );
    }
    let mut selected = request
        .names
        .iter()
        .enumerate()
        .filter(|(_, name)| page_number(name, true, true).is_some())
        .collect::<Vec<_>>();
    selected.sort_by_cached_key(|(_, name)| name.to_ascii_lowercase());
    let pages = selected
        .into_iter()
        .map(|(index, name)| LocalPage {
            source_index: u32::try_from(index).expect("bounded entry count"),
            extension: name
                .rsplit_once('.')
                .expect("selected page extension")
                .1
                .to_owned(),
        })
        .collect();
    ApiEnvelope::ok(1, "localPages", pages)
}

#[cfg(test)]
mod tests;
