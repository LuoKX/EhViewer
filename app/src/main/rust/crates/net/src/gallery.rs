//! Gallery catalog and detail protocols.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const PUBLIC_HOST: &str = "e-hentai.org";
const RESTRICTED_HOST: &str = "exhentai.org";

/// Gallery site shared by gallery request planners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GallerySite {
    /// Public e-hentai site.
    #[serde(rename = "public")]
    Public,
    /// Restricted exhentai site.
    #[serde(rename = "exHentai")]
    ExHentai,
}

impl GallerySite {
    pub(crate) fn host(self) -> &'static str {
        match self {
            Self::Public => PUBLIC_HOST,
            Self::ExHentai => RESTRICTED_HOST,
        }
    }
}

/// Ordered HTML form field shared by gallery planners.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormField {
    /// Field name.
    pub name: String,
    /// Opaque field value.
    pub value: String,
}

pub mod archive;
pub mod archive_request;
pub mod comment_vote;
pub mod comments;
pub mod detail;
pub mod favorite_mutation;
pub mod favorites;
pub mod favorites_url;
pub mod list;
pub mod list_url;
pub mod list_url_parser;
pub mod metadata;
pub mod navigation;
pub mod network_policy;
pub mod preview;
pub mod rate;
pub mod tag_vote;
mod tags;
pub mod token;

/// Reads a signed integer from either a JSON number or the quoted numeric form
/// accepted by the legacy Kotlin serializer.
fn legacy_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// Reads a floating-point value from either a JSON number or the quoted numeric
/// form accepted by the legacy Kotlin serializer.
fn legacy_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}
