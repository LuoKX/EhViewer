//! Pure Gallery Archive request planning.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{FormField, GallerySite};

/// Archive request protocol version.
pub const API_VERSION: u8 = 1;
/// Stable module identifier.
pub const MODULE: &str = "galleryArchiveRequest";

const UNSUPPORTED_VERSION: &str = "unsupported gallery archive request protocol version";
const INVALID_SHAPE: &str = "invalid gallery archive request kind or optional-field shape";
const INTERNAL_FAILURE: &str = "gallery archive request planning failed";

/// Request operation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveRequestKind {
    /// Request archive list HTML.
    List,
    /// Request one archive download.
    Download,
}

/// Versioned request input. Opaque values are preserved exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryArchiveRequest {
    /// Protocol version.
    pub version: u8,
    /// Gallery site.
    pub site: GallerySite,
    /// List or download.
    pub kind: ArchiveRequestKind,
    /// Signed gallery identifier.
    pub gid: i64,
    /// Opaque gallery token.
    pub token: String,
    /// Resolution, required only for downloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// H@H selection, required only for downloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_hath: Option<bool>,
}

/// Deterministic request plan consumed by Android before HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryArchiveRequestPlan {
    /// Operation kind.
    pub kind: ArchiveRequestKind,
    /// Exact request URL.
    pub url: String,
    /// Exact gallery-detail referer URL.
    pub referer: String,
    /// Nullable Origin header.
    pub origin: Option<String>,
    /// Ordered form fields.
    pub form: Vec<FormField>,
}

/// Plans a Gallery Archive request without network or platform side effects.
///
/// # Errors
///
/// Returns a fixed, sanitized error when the operation kind and optional fields
/// disagree or when an internal URL cannot be constructed.
pub fn plan(request: &GalleryArchiveRequest) -> Result<GalleryArchiveRequestPlan, &'static str> {
    let valid_shape = match request.kind {
        ArchiveRequestKind::List => request.resolution.is_none() && request.is_hath.is_none(),
        ArchiveRequestKind::Download => request.resolution.is_some() && request.is_hath.is_some(),
    };
    if !valid_shape {
        return Err(INVALID_SHAPE);
    }
    let host = request.site.host();
    let mut url =
        Url::parse(&format!("https://{host}/archiver.php")).map_err(|_| INTERNAL_FAILURE)?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("gid", &request.gid.to_string());
        query.append_pair("token", &request.token);
    }
    let url = url.to_string();
    let mut referer_url = Url::parse(&format!("https://{host}/")).map_err(|_| INTERNAL_FAILURE)?;
    {
        let mut segments = referer_url
            .path_segments_mut()
            .map_err(|()| INTERNAL_FAILURE)?;
        segments
            .push("g")
            .push(&request.gid.to_string())
            .push(&request.token);
    }
    let referer = referer_url.to_string();
    match request.kind {
        ArchiveRequestKind::List => Ok(GalleryArchiveRequestPlan {
            kind: request.kind,
            url,
            referer,
            origin: None,
            form: Vec::new(),
        }),
        ArchiveRequestKind::Download => {
            let resolution = request.resolution.as_deref().ok_or(INVALID_SHAPE)?;
            let is_hath = request.is_hath.ok_or(INVALID_SHAPE)?;
            let form = if is_hath {
                vec![FormField {
                    name: "hathdl_xres".into(),
                    value: resolution.into(),
                }]
            } else {
                let mut fields = vec![FormField {
                    name: "dltype".into(),
                    value: resolution.into(),
                }];
                fields.push(FormField {
                    name: "dlcheck".into(),
                    value: if resolution == "org" {
                        "Download Original Archive"
                    } else {
                        "Download Resample Archive"
                    }
                    .into(),
                });
                fields
            };
            Ok(GalleryArchiveRequestPlan {
                kind: request.kind,
                url,
                referer,
                origin: Some(format!("https://{host}")),
                form,
            })
        }
    }
}

/// Builds the stable success/error envelope for Android.
#[must_use]
pub fn build_envelope(request: &GalleryArchiveRequest) -> ApiEnvelope<GalleryArchiveRequestPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error(UNSUPPORTED_VERSION.into()),
        );
    }
    match plan(request) {
        Ok(plan) => ApiEnvelope::ok(API_VERSION, MODULE, plan),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, RustError::Error(error.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST_REQUEST_CBOR_HEX: &str = concat!(
        "a56776657273696f6e01647369746568657848656e746169646b696e64646c69737463676964",
        "2665746f6b656e646120623f"
    );
    const LIST_RESPONSE_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c657567616c6c65727941726368697665526571756573",
        "74677061796c6f6164a5646b696e64646c6973746375726c783568747470733a2f2f65786865",
        "6e7461692e6f72672f61726368697665722e7068703f6769643d2d3726746f6b656e3d612b62",
        "2533466772656665726572782268747470733a2f2f657868656e7461692e6f72672f672f2d37",
        "2f6125323062253346666f726967696ef664666f726d80656572726f72f6"
    );
    const DOWNLOAD_REQUEST_CBOR_HEX: &str = concat!(
        "a76776657273696f6e016473697465667075626c6963646b696e6468646f776e6c6f61646367",
        "69642665746f6b656e646120623f6a7265736f6c7574696f6e636f726766697348617468f4"
    );
    const DOWNLOAD_RESPONSE_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c657567616c6c65727941726368697665526571756573",
        "74677061796c6f6164a5646b696e6468646f776e6c6f61646375726c783568747470733a2f2f",
        "652d68656e7461692e6f72672f61726368697665722e7068703f6769643d2d3726746f6b656e",
        "3d612b622533466772656665726572782268747470733a2f2f652d68656e7461692e6f72672f",
        "672f2d372f6125323062253346666f726967696e7468747470733a2f2f652d68656e7461692e",
        "6f726764666f726d82a2646e616d6566646c747970656576616c7565636f7267a2646e616d65",
        "67646c636865636b6576616c75657819446f776e6c6f6164204f726967696e616c2041726368",
        "697665656572726f72f6"
    );

    #[test]
    fn list_plans_cover_both_sites_and_encode_opaque_values() {
        let cases = [
            (
                GallerySite::Public,
                "https://e-hentai.org/archiver.php?gid=-7&token=a+b%3F",
                "https://e-hentai.org/g/-7/a%20b%3F",
            ),
            (
                GallerySite::ExHentai,
                "https://exhentai.org/archiver.php?gid=-7&token=a+b%3F",
                "https://exhentai.org/g/-7/a%20b%3F",
            ),
        ];

        for (site, url, referer) in cases {
            assert_eq!(
                planned(&request(site, ArchiveRequestKind::List, None, None)),
                GalleryArchiveRequestPlan {
                    kind: ArchiveRequestKind::List,
                    url: url.into(),
                    referer: referer.into(),
                    origin: None,
                    form: Vec::new(),
                }
            );
        }
    }

    #[test]
    fn direct_and_hath_forms_preserve_legacy_order_and_values() {
        let cases = [
            (
                GallerySite::Public,
                "org",
                false,
                vec![
                    field("dltype", "org"),
                    field("dlcheck", "Download Original Archive"),
                ],
            ),
            (
                GallerySite::ExHentai,
                "1280 weird/?",
                false,
                vec![
                    field("dltype", "1280 weird/?"),
                    field("dlcheck", "Download Resample Archive"),
                ],
            ),
            (
                GallerySite::Public,
                "",
                false,
                vec![
                    field("dltype", ""),
                    field("dlcheck", "Download Resample Archive"),
                ],
            ),
            (
                GallerySite::ExHentai,
                "1600",
                true,
                vec![field("hathdl_xres", "1600")],
            ),
            (
                GallerySite::Public,
                "",
                true,
                vec![field("hathdl_xres", "")],
            ),
        ];

        for (site, resolution, is_hath, form) in cases {
            let host = site.host();
            let planned = planned(&request(
                site,
                ArchiveRequestKind::Download,
                Some(resolution),
                Some(is_hath),
            ));
            assert_eq!(planned.kind, ArchiveRequestKind::Download);
            assert_eq!(
                planned.url,
                format!("https://{host}/archiver.php?gid=-7&token=a+b%3F")
            );
            assert_eq!(planned.referer, format!("https://{host}/g/-7/a%20b%3F"));
            assert_eq!(planned.origin, Some(format!("https://{host}")));
            assert_eq!(planned.form, form);
        }
    }

    #[test]
    fn every_invalid_optional_field_shape_returns_one_fixed_error() {
        let cases = [
            (ArchiveRequestKind::List, Some("org"), None),
            (ArchiveRequestKind::List, None, Some(false)),
            (ArchiveRequestKind::List, Some("org"), Some(false)),
            (ArchiveRequestKind::Download, None, None),
            (ArchiveRequestKind::Download, Some("org"), None),
            (ArchiveRequestKind::Download, None, Some(false)),
        ];

        for (kind, resolution, is_hath) in cases {
            let request = request(GallerySite::Public, kind, resolution, is_hath);
            assert_eq!(plan(&request), Err(INVALID_SHAPE));
            assert_eq!(
                build_envelope(&request).error,
                Some(RustError::Error(INVALID_SHAPE.into()))
            );
        }
    }

    #[test]
    fn envelopes_are_versioned_and_never_mix_payload_with_error() {
        let request = request(GallerySite::Public, ArchiveRequestKind::List, None, None);
        let envelope = build_envelope(&request);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert!(envelope.payload.is_some());
        assert_eq!(envelope.error, None);

        let unsupported = GalleryArchiveRequest {
            version: API_VERSION + 1,
            ..request
        };
        let envelope = build_envelope(&unsupported);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(envelope.payload, None);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(UNSUPPORTED_VERSION.into()))
        );
    }

    #[test]
    fn json_and_cbor_goldens_freeze_optional_fields_and_plan_envelopes() {
        let list = request(GallerySite::ExHentai, ArchiveRequestKind::List, None, None);
        let download = request(
            GallerySite::Public,
            ArchiveRequestKind::Download,
            Some("org"),
            Some(false),
        );
        assert_eq!(
            serde_json::to_string(&list).expect("list request must encode as JSON"),
            r#"{"version":1,"site":"exHentai","kind":"list","gid":-7,"token":"a b?"}"#
        );
        assert_eq!(
            serde_json::to_string(&download).expect("download request must encode as JSON"),
            r#"{"version":1,"site":"public","kind":"download","gid":-7,"token":"a b?","resolution":"org","isHath":false}"#
        );
        assert_eq!(
            cbor_hex(&list),
            LIST_REQUEST_CBOR_HEX,
            "list request CBOR drifted"
        );
        assert_eq!(
            cbor_hex(&build_envelope(&list)),
            LIST_RESPONSE_CBOR_HEX,
            "list response CBOR drifted"
        );
        assert_eq!(
            cbor_hex(&download),
            DOWNLOAD_REQUEST_CBOR_HEX,
            "download request CBOR drifted"
        );
        assert_eq!(
            cbor_hex(&build_envelope(&download)),
            DOWNLOAD_RESPONSE_CBOR_HEX,
            "download response CBOR drifted"
        );
    }

    fn request(
        site: GallerySite,
        kind: ArchiveRequestKind,
        resolution: Option<&str>,
        is_hath: Option<bool>,
    ) -> GalleryArchiveRequest {
        GalleryArchiveRequest {
            version: API_VERSION,
            site,
            kind,
            gid: -7,
            token: "a b?".into(),
            resolution: resolution.map(str::to_owned),
            is_hath,
        }
    }

    fn planned(request: &GalleryArchiveRequest) -> GalleryArchiveRequestPlan {
        plan(request).expect("valid request must produce a plan")
    }

    fn field(name: &str, value: &str) -> FormField {
        FormField {
            name: name.into(),
            value: value.into(),
        }
    }

    fn cbor_hex(value: &impl Serialize) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let bytes = serde_cbor::to_vec(value).expect("wire value must encode as CBOR");
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}
