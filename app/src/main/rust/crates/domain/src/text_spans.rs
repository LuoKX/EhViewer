//! Batched UTF-16 text-span normalization; Compose styles and text remain Android-owned.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

const MAX_SPANS: usize = 100_000;
const MAX_TEXT_LENGTH: u32 = i32::MAX as u32;

/// One observed half-open UTF-16 span, potentially carrying multiple recognized styles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpan {
    /// Inclusive UTF-16 start.
    pub start: u32,
    /// Exclusive UTF-16 end.
    pub end: u32,
    /// Bold, italic, underline and strikethrough bits (1, 2, 4 and 8).
    pub flags: u8,
}

/// Bounded V1 observation; no comment text crosses JNI.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpansRequest {
    /// Contract version.
    pub version: u8,
    /// Java text length in UTF-16 units.
    pub text_length: u32,
    /// All original spans in source order, including unsupported/empty observations.
    pub spans: Vec<TextSpan>,
}

/// Normalizes each style independently, preserving the old style-group order.
#[must_use]
pub fn build_envelope(request: &TextSpansRequest) -> ApiEnvelope<Vec<TextSpan>> {
    if request.version != 1
        || request.text_length > MAX_TEXT_LENGTH
        || request.spans.len() > MAX_SPANS
        || request
            .spans
            .iter()
            .any(|s| s.start > s.end || s.end > request.text_length || s.flags > 15)
    {
        return ApiEnvelope::err(
            1,
            "textSpans",
            RustError::Error("Invalid text spans input".into()),
        );
    }
    let mut output = Vec::new();
    for flag in [1, 2, 4, 8] {
        let mut group = request
            .spans
            .iter()
            .filter(|s| s.flags & flag != 0 && s.start != s.end)
            .copied()
            .collect::<Vec<_>>();
        group.sort_by_key(|s| (s.start, s.end));
        let mut current: Option<TextSpan> = None;
        for span in group {
            match current.as_mut() {
                Some(merged) if span.start <= merged.end => merged.end = merged.end.max(span.end),
                _ => {
                    if let Some(previous) = current.take() {
                        output.push(previous);
                    }
                    current = Some(TextSpan {
                        flags: flag,
                        ..span
                    });
                }
            }
        }
        if let Some(previous) = current {
            output.push(previous);
        }
    }
    ApiEnvelope::ok(1, "textSpans", output)
}

#[cfg(test)]
mod tests;
