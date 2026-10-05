//! Portable short-language inference and Gallery metadata enrichment eligibility.
//! Android retains settings, model mutation and HTTP.

use crate::contracts::{ApiEnvelope, RustError};
use regex_lite::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// V1 enrichment protocol version.
pub const API_VERSION: u8 = 1;
/// Stable enrichment envelope discriminator.
pub const MODULE: &str = "galleryEnrichment";

/// Pure enrichment operation.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EnrichmentKind {
    /// Infer a display language from tags, then the title.
    Language,
    /// Decide whether Gallery List needs an extra metadata request.
    MetadataNeeded,
}

/// Android-observed eligibility facts; no settings or filters run in Rust.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent Android-observed eligibility facts, not mutable lifecycle state"
)]
pub struct MetadataFacts {
    /// Whether post-enrichment filtering is requested.
    pub filter: bool,
    /// Whether the pre-enrichment filter needs tags.
    pub needs_tags: bool,
    /// Whether any current gallery has tags.
    pub has_tags: bool,
    /// Whether page counts should be shown.
    pub show_pages: bool,
    /// Whether any current gallery has a page count.
    pub has_pages: bool,
    /// Whether any gallery has already been rated.
    pub has_rated: bool,
}

/// V1 inference/eligibility input; absent optional fields retain their meaning.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentRequest {
    /// Contract version.
    pub version: u8,
    /// Selected operation.
    pub kind: EnrichmentKind,
    /// Optional title used only when no recognized tag supplies a language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Source-ordered, case-sensitive tags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Platform eligibility snapshot for the metadata operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facts: Option<MetadataFacts>,
}

/// Exactly one operation result; unknown language is a successful null value.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentPlan {
    /// Operation echoed for strict Android validation.
    pub kind: EnrichmentKind,
    /// Recognized display language or null.
    pub language: Option<String>,
    /// Eligibility result, populated only for metadata requests.
    pub needs_metadata: Option<bool>,
}

const CODES: [&str; 14] = [
    "EN", "ZH", "ES", "KO", "RU", "FR", "PT", "TH", "DE", "IT", "VI", "PL", "HU", "NL",
];
const TAGS: [&str; 14] = [
    "english",
    "chinese",
    "spanish",
    "korean",
    "russian",
    "french",
    "portuguese",
    "thai",
    "german",
    "italian",
    "vietnamese",
    "polish",
    "hungarian",
    "dutch",
];
const PATTERNS: [&str; 14] = [
    r"[(\[]eng(?:lish)?[)\]]|英訳",
    r"[(（\[]ch(?:inese)?[)）\]]|[汉漢]化|中[国國][语語]|中文|中国翻訳",
    r"[(\[]spanish[)\]]|[(\[]español[)\]]|スペイン翻訳",
    r"[(\[]korean?[)\]]|韓国翻訳",
    r"[(\[]rus(?:sian)?[)\]]|ロシア翻訳",
    r"[(\[]fr(?:ench)?[)\]]|フランス翻訳",
    r"[(\[]portuguese|ポルトガル翻訳",
    r"[(\[]thai(?: ภาษาไทย)?[)\]]|แปลไทย|タイ翻訳",
    r"[(\[]german[)\]]|ドイツ翻訳",
    r"[(\[]italiano?[)\]]|イタリア翻訳",
    r"[(\[]vietnamese(?: tiếng việt)?[)\]]|ベトナム翻訳",
    r"[(\[]polish[)\]]|ポーランド翻訳",
    r"[(\[]hun(?:garian)?[)\]]|ハンガリー翻訳",
    r"[(\[]dutch[)\]]|オランダ翻訳",
];
static TITLE_PATTERNS: LazyLock<[Regex; 14]> = LazyLock::new(|| {
    PATTERNS.map(|pattern| Regex::new(pattern).expect("source-controlled language pattern"))
});

/// Infers tags in source order, then title patterns in the historical language priority order.
#[must_use]
pub fn infer_language(title: Option<&str>, tags: Option<&[String]>) -> Option<String> {
    if let Some(tags) = tags {
        for tag in tags {
            if let Some(index) = tag
                .strip_prefix("language:")
                .and_then(|name| TAGS.iter().position(|known| *known == name))
            {
                return Some(CODES[index].to_owned());
            }
        }
    }
    let title = title?;
    // regex-lite has ASCII-only case folding. Apply Java-compatible simple Unicode folding first.
    let title: String = title
        .chars()
        .map(|value| {
            let mut upper = value.to_uppercase();
            let first = upper.next().unwrap_or(value);
            let upper = if upper.next().is_none() { first } else { value };
            upper.to_lowercase().next().unwrap_or(upper)
        })
        .collect();
    TITLE_PATTERNS
        .iter()
        .position(|pattern| pattern.is_match(&title))
        .map(|index| CODES[index].to_owned())
}

/// Builds one versioned result without credentials, effects or fallback.
#[must_use]
pub fn build_envelope(request: &EnrichmentRequest) -> ApiEnvelope<EnrichmentPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error("Unsupported gallery enrichment version".to_owned()),
        );
    }
    match request.kind {
        EnrichmentKind::Language if request.facts.is_none() => ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            EnrichmentPlan {
                kind: request.kind,
                language: infer_language(request.title.as_deref(), request.tags.as_deref()),
                needs_metadata: None,
            },
        ),
        EnrichmentKind::MetadataNeeded if request.title.is_none() && request.tags.is_none() => {
            let Some(facts) = &request.facts else {
                return invalid();
            };
            ApiEnvelope::ok(
                API_VERSION,
                MODULE,
                EnrichmentPlan {
                    kind: request.kind,
                    language: None,
                    needs_metadata: Some(
                        (facts.filter && facts.needs_tags && !facts.has_tags)
                            || (facts.show_pages && !facts.has_pages)
                            || facts.has_rated,
                    ),
                },
            )
        }
        _ => invalid(),
    }
}
fn invalid() -> ApiEnvelope<EnrichmentPlan> {
    ApiEnvelope::err(
        API_VERSION,
        MODULE,
        RustError::Error("Invalid gallery enrichment shape".to_owned()),
    )
}

#[cfg(test)]
mod tests;
