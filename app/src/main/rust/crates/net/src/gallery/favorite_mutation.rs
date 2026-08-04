//! Pure request planning for Gallery Favorites mutations.
//!
//! This module owns only the versioned URL and ordered form-field shape. Android
//! retains HTTP, cookies, response parsing, refresh, persistence, and all effects.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Backwards-compatible source name for the shared form field.
pub use super::FormField;
/// Backwards-compatible source name for the shared gallery site.
pub use super::GallerySite as FavoritesSite;

/// V1 protocol version for favorite mutation plans.
pub const API_VERSION: u8 = 1;
/// Stable module identifier for the mutation planner.
pub const MODULE: &str = "galleryFavoriteMutation";

const INVALID_DESTINATION: &str = "invalid favorite mutation destination";
const INVALID_OPERATION: &str = "invalid favorite mutation operation";
const UNSUPPORTED_VERSION: &str = "unsupported favorite mutation protocol version";

/// The selected mutation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationKind {
    /// One gallery favorite add/delete/note operation.
    Single,
    /// One bulk move/delete operation.
    Bulk,
}

/// Complete request plan consumed by Android before HTTP starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteMutationPlan {
    /// Selected operation shape.
    pub kind: OperationKind,
    /// Exact HTTPS request target.
    pub url: String,
    /// Referer header value.
    pub referer: String,
    /// Origin header value.
    pub origin: String,
    /// Ordered form fields, including repeated names.
    pub form: Vec<FormField>,
}

/// One single or bulk mutation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteMutationRequest {
    /// Protocol version; must equal [`API_VERSION`].
    pub version: u8,
    /// Site selected by Android settings.
    pub site: FavoritesSite,
    /// Single operation payload, mutually exclusive with [`Self::bulk`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub single: Option<FavoriteMutationSingle>,
    /// Bulk operation payload, mutually exclusive with [`Self::single`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bulk: Option<FavoriteMutationBulk>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Fields for one single mutation.
pub struct FavoriteMutationSingle {
    /// Signed gallery identifier.
    pub gid: i64,
    /// Opaque gallery token.
    pub token: String,
    /// Destination slot, or `-1` to remove.
    pub destination: i32,
    /// Opaque note value.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Fields for one bulk mutation.
pub struct FavoriteMutationBulk {
    /// Source-ordered signed IDs.
    pub ids: Vec<i64>,
    /// Source category.
    pub source: i32,
    /// Destination slot, or `-1` to delete.
    pub destination: i32,
}

fn operation(
    request: &FavoriteMutationRequest,
) -> Result<(OperationKind, String, String, String, Vec<FormField>), &'static str> {
    match (&request.single, &request.bulk) {
        (Some(single), None) => {
            let host = request.site.host();
            let origin = format!("https://{host}");
            let url = format!(
                "https://{host}/gallerypopups.php?gid={}&t={}&act=addfav",
                single.gid,
                encode(&single.token)
            );
            let favcat = match single.destination {
                -1 => "favdel".to_owned(),
                0..=9 => single.destination.to_string(),
                _ => return Err(INVALID_DESTINATION),
            };
            Ok((
                OperationKind::Single,
                url.clone(),
                url,
                origin,
                vec![
                    field("favcat", favcat),
                    field("favnote", single.note.clone()),
                    field("apply", "Apply Changes"),
                    field("update", "1"),
                ],
            ))
        }
        (None, Some(bulk)) => {
            let host = request.site.host();
            let origin = format!("https://{host}");
            let mut url = format!("https://{host}/favorites.php");
            if (0..=9).contains(&bulk.source) {
                url.push_str("?favcat=");
                url.push_str(&bulk.source.to_string());
            }
            let ddact = match bulk.destination {
                -1 => "delete".to_owned(),
                0..=9 => format!("fav{}", bulk.destination),
                _ => return Err(INVALID_DESTINATION),
            };
            let mut form = vec![field("ddact", ddact)];
            form.extend(
                bulk.ids
                    .iter()
                    .map(|id| field("modifygids[]", id.to_string())),
            );
            Ok((OperationKind::Bulk, url.clone(), url, origin, form))
        }
        _ => Err(INVALID_OPERATION),
    }
}

/// Builds a versioned success/error envelope without platform or network access.
#[must_use]
pub fn build_envelope(request: &FavoriteMutationRequest) -> ApiEnvelope<FavoriteMutationPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error(UNSUPPORTED_VERSION.into()),
        );
    }
    match build(request) {
        Ok(plan) => ApiEnvelope::ok(API_VERSION, MODULE, plan),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, RustError::Error(error.into())),
    }
}

fn build(request: &FavoriteMutationRequest) -> Result<FavoriteMutationPlan, &'static str> {
    let (kind, url, referer, origin, form) = operation(request)?;
    Ok(FavoriteMutationPlan {
        kind,
        url,
        referer,
        origin,
        form,
    })
}

fn field(name: &str, value: impl Into<String>) -> FormField {
    FormField {
        name: name.to_owned(),
        value: value.into(),
    }
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn site() -> FavoritesSite {
        FavoritesSite::Public
    }

    #[test]
    fn single_preserves_order_and_opaque_values() {
        let request = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: Some(FavoriteMutationSingle {
                gid: -7,
                token: "a b?".into(),
                destination: -1,
                note: String::new(),
            }),
            bulk: None,
        };
        let plan = build_envelope(&request).payload.expect("single plan");
        assert_eq!(
            plan.url,
            "https://e-hentai.org/gallerypopups.php?gid=-7&t=a+b%3F&act=addfav"
        );
        assert_eq!(plan.form[0].value, "favdel");
        assert_eq!(plan.form[1].value, "");
    }

    #[test]
    fn bulk_preserves_signed_ids_and_source_omission() {
        let request = FavoriteMutationRequest {
            version: 1,
            site: FavoritesSite::ExHentai,
            single: None,
            bulk: Some(FavoriteMutationBulk {
                ids: vec![-2, 0, 4],
                source: -3,
                destination: 9,
            }),
        };
        let plan = build_envelope(&request).payload.expect("bulk plan");
        assert_eq!(plan.url, "https://exhentai.org/favorites.php");
        assert_eq!(
            plan.form
                .iter()
                .map(|f| f.value.as_str())
                .collect::<Vec<_>>(),
            vec!["fav9", "-2", "0", "4"]
        );
    }

    #[test]
    fn failures_are_fixed_and_version_is_checked() {
        let invalid_single = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: Some(FavoriteMutationSingle {
                gid: 1,
                token: "x".into(),
                destination: 99,
                note: String::new(),
            }),
            bulk: None,
        };
        assert_eq!(
            build_envelope(&invalid_single).error,
            Some(RustError::Error(INVALID_DESTINATION.into()))
        );
        let bad = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: None,
            bulk: Some(FavoriteMutationBulk {
                ids: vec![],
                source: 0,
                destination: 99,
            }),
        };
        assert_eq!(
            build_envelope(&bad).error,
            Some(RustError::Error(INVALID_DESTINATION.into()))
        );
        let empty_bulk = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: None,
            bulk: Some(FavoriteMutationBulk {
                ids: vec![],
                source: 0,
                destination: -1,
            }),
        };
        let empty_plan = build_envelope(&empty_bulk)
            .payload
            .expect("empty bulk plan");
        assert_eq!(
            empty_plan.form,
            vec![FormField {
                name: "ddact".into(),
                value: "delete".into()
            }]
        );
        let wrong = FavoriteMutationRequest { version: 2, ..bad };
        assert_eq!(
            build_envelope(&wrong).error,
            Some(RustError::Error(UNSUPPORTED_VERSION.into()))
        );
        let neither = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: None,
            bulk: None,
        };
        assert_eq!(
            build_envelope(&neither).error,
            Some(RustError::Error(INVALID_OPERATION.into()))
        );
        let both = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: Some(FavoriteMutationSingle {
                gid: 1,
                token: "x".into(),
                destination: 0,
                note: String::new(),
            }),
            bulk: Some(FavoriteMutationBulk {
                ids: vec![],
                source: 0,
                destination: -1,
            }),
        };
        assert_eq!(
            build_envelope(&both).error,
            Some(RustError::Error(INVALID_OPERATION.into()))
        );
    }

    #[test]
    fn single_decimal_destination_and_special_values_are_preserved() {
        let request = FavoriteMutationRequest {
            version: 1,
            site: site(),
            single: Some(FavoriteMutationSingle {
                gid: 8,
                token: " %?+日本".into(),
                destination: 4,
                note: "=&".into(),
            }),
            bulk: None,
        };
        let plan = build_envelope(&request).payload.expect("single");
        assert!(plan.url.contains("t=+%25%3F%2B%E6%97%A5%E6%9C%AC"));
        assert_eq!(plan.form[0].value, "4");
        assert_eq!(plan.form[1].value, "=&");
    }

    #[test]
    fn json_and_cbor_shapes_are_stable() {
        let request = FavoriteMutationRequest {
            version: API_VERSION,
            site: FavoritesSite::Public,
            single: None,
            bulk: Some(FavoriteMutationBulk {
                ids: vec![-1, 2],
                source: 3,
                destination: -1,
            }),
        };
        let json = serde_json::to_value(&request).expect("request JSON");
        assert_eq!(
            json,
            serde_json::json!({"version":1,"site":"public","bulk":{"ids":[-1,2],"source":3,"destination":-1}})
        );
        let bytes = serde_cbor::to_vec(&request).expect("request CBOR");
        assert_eq!(
            to_hex(&bytes),
            "a36776657273696f6e016473697465667075626c69636462756c6ba36369647382200266736f75726365036b64657374696e6174696f6e20"
        );
        let decoded: FavoriteMutationRequest =
            serde_cbor::from_slice(&bytes).expect("request CBOR decode");
        assert_eq!(decoded, request);
        let envelope = build_envelope(&request);
        let response = serde_cbor::to_vec(&envelope).expect("response CBOR");
        assert_eq!(
            to_hex(&response),
            "a46776657273696f6e01666d6f64756c657767616c6c6572794661766f726974654d75746174696f6e677061796c6f6164a5646b696e646462756c6b6375726c782b68747470733a2f2f652d68656e7461692e6f72672f6661766f72697465732e7068703f6661766361743d336772656665726572782b68747470733a2f2f652d68656e7461692e6f72672f6661766f72697465732e7068703f6661766361743d33666f726967696e7468747470733a2f2f652d68656e7461692e6f726764666f726d83a2646e616d656564646163746576616c75656664656c657465a2646e616d656c6d6f64696679676964735b5d6576616c7565622d31a2646e616d656c6d6f64696679676964735b5d6576616c75656132656572726f72f6"
        );
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut out = String::new();
        for byte in bytes {
            write!(out, "{byte:02x}").expect("string");
        }
        out
    }
}
