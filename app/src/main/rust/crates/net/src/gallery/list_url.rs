//! Pure gallery-catalog URL construction.
//!
//! Android retains the mutable, serializable list-navigation state and selects
//! the active site. This module owns only the remote URL protocol: paths,
//! ordered query parameters, category inversion, and search-prefix rules. No
//! cookies, gallery tokens, response bodies, or platform state cross this API.

use eh_domain::contracts::{ApiEnvelope, RustError};
use eh_domain::gallery_rules::{CATEGORY_NON_H, invert_category};
use serde::{Deserialize, Serialize};
use url::Url;

const PUBLIC_HOST: &str = "e-hentai.org";
const RESTRICTED_HOST: &str = "exhentai.org";
const ADVANCED_SEARCH_SH: i32 = 1;
const ADVANCED_SEARCH_STO: i32 = 2;
const ADVANCED_SEARCH_SFL: i32 = 4;
const ADVANCED_SEARCH_SFU: i32 = 8;
const ADVANCED_SEARCH_SFT: i32 = 16;

/// Current gallery-list URL bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable gallery-list URL module discriminator used by Android adapters.
pub const MODULE: &str = "galleryListUrl";

/// Gallery site selected by Android settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryListSite {
    /// Public `e-hentai.org` catalog.
    Public,
    /// Restricted `exhentai.org` catalog.
    ExHentai,
}

impl GalleryListSite {
    fn host(self) -> &'static str {
        match self {
            Self::Public => PUBLIC_HOST,
            Self::ExHentai => RESTRICTED_HOST,
        }
    }
}

/// Legacy catalog URL mode represented as a stable wire enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryListMode {
    /// Main gallery catalog.
    Normal,
    /// Galleries from one uploader.
    Uploader,
    /// Galleries carrying one tag.
    Tag,
    /// Site-specific popular catalog.
    WhatsHot,
    /// Reverse image-search form.
    ImageSearch,
    /// Watched-tag subscription catalog.
    Subscription,
    /// Public-site toplist.
    Toplist,
}

/// Complete sanitized state needed to construct one gallery-list URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlRequest {
    /// Protocol version; callers must use [`API_VERSION`].
    pub version: u8,
    /// Gallery host selected by Android.
    pub site: GalleryListSite,
    /// Whether Android currently has an authenticated gallery session.
    pub signed_in: bool,
    /// Catalog mode to construct.
    pub mode: GalleryListMode,
    /// Previous-page cursor, omitted when blank.
    pub previous: Option<String>,
    /// Next-page cursor, omitted when blank.
    pub next: Option<String>,
    /// Date/toplist seek cursor, omitted when blank.
    pub seek: Option<String>,
    /// Legacy range value; emitted only when positive.
    pub range: i32,
    /// Android category bit mask before remote-protocol inversion.
    pub category: i32,
    /// Search, uploader, tag, or toplist value depending on [`Self::mode`].
    pub keyword: Option<String>,
    /// Reverse image-search hash.
    pub image_hash: Option<String>,
    /// Already selected `language:*` tag, or `None` when no filter is active.
    pub language_tag: Option<String>,
    /// Legacy advanced-search option bit mask.
    pub advanced_search: i32,
    /// Minimum rating filter; emitted only when positive.
    pub minimum_rating: i32,
    /// Minimum page-count filter; emitted only when positive.
    pub page_from: i32,
    /// Maximum page-count filter; emitted only when positive.
    pub page_to: i32,
}

/// Versioned output payload containing one fully encoded request target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryListUrlPlan {
    /// HTTPS URL with legacy-compatible path and ordered query parameters.
    pub url: String,
}

/// Builds a versioned success/error envelope for the Android bridge.
#[must_use]
pub fn build_envelope(request: &GalleryListUrlRequest) -> ApiEnvelope<GalleryListUrlPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error("unsupported gallery-list URL protocol version".to_owned()),
        );
    }
    match build(request) {
        Ok(plan) => ApiEnvelope::ok(API_VERSION, MODULE, plan),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Constructs one gallery-list URL without accessing platform or session state.
///
/// # Errors
///
/// Returns [`RustError::Error`] when a legacy mode requires a missing keyword or
/// image hash. Field values are deliberately excluded from error messages.
pub fn build(request: &GalleryListUrlRequest) -> Result<GalleryListUrlPlan, RustError> {
    let url = match request.mode {
        GalleryListMode::Normal | GalleryListMode::Subscription => {
            build_normal_or_subscription(request)
        }
        GalleryListMode::Uploader => build_named_catalog(request, "uploader")?,
        GalleryListMode::Tag => build_named_catalog(request, "tag")?,
        GalleryListMode::WhatsHot => base_url(request.site.host(), &["popular"]),
        GalleryListMode::ImageSearch => build_image_search(request)?,
        GalleryListMode::Toplist => build_toplist(request)?,
    };
    Ok(GalleryListUrlPlan { url: url.into() })
}

fn build_normal_or_subscription(request: &GalleryListUrlRequest) -> Url {
    let path = if request.mode == GalleryListMode::Subscription {
        &["watched"][..]
    } else {
        &[]
    };
    let mut url = base_url(request.site.host(), path);
    let category = if !request.signed_in && request.category <= 0 {
        CATEGORY_NON_H
    } else {
        request.category
    };
    if category > 0 {
        append(&mut url, "f_cats", &invert_category(category).to_string());
    }
    if let Some(query) = prefixed_query(request) {
        append_non_blank(&mut url, "f_search", Some(&query));
    }
    append_navigation(&mut url, request);
    append_advanced_search(&mut url, request);
    url
}

fn build_named_catalog(request: &GalleryListUrlRequest, namespace: &str) -> Result<Url, RustError> {
    let keyword = required(
        request.keyword.as_deref(),
        "gallery-list URL keyword is missing",
    )?;
    let mut url = base_url(request.site.host(), &[namespace, keyword]);
    append_navigation(&mut url, request);
    Ok(url)
}

fn build_image_search(request: &GalleryListUrlRequest) -> Result<Url, RustError> {
    let image_hash = required(
        request.image_hash.as_deref(),
        "gallery-list image-search hash is missing",
    )?;
    let mut url = base_url(request.site.host(), &[]);
    append(&mut url, "f_shash", image_hash);
    Ok(url)
}

fn build_toplist(request: &GalleryListUrlRequest) -> Result<Url, RustError> {
    let toplist = required(
        request.keyword.as_deref(),
        "gallery-list toplist value is missing",
    )?;
    let mut url = base_url(PUBLIC_HOST, &["toplist.php"]);
    append(&mut url, "tl", toplist);
    append_non_blank(&mut url, "p", request.seek.as_deref());
    Ok(url)
}

fn base_url(host: &str, path: &[&str]) -> Url {
    let mut url = Url::parse(&format!("https://{host}/"))
        .expect("source-controlled gallery hosts must form valid HTTPS URLs");
    if !path.is_empty() {
        let mut segments = url
            .path_segments_mut()
            .expect("an HTTPS gallery URL must support path segments");
        segments.pop_if_empty();
        segments.extend(path.iter().copied());
    }
    url
}

fn prefixed_query(request: &GalleryListUrlRequest) -> Option<String> {
    request.keyword.as_ref().map(|keyword| {
        let has_language_operator =
            keyword.contains("gid:") || keyword.contains("l:") || keyword.contains("language:");
        match request.language_tag.as_deref() {
            Some(tag) if !has_language_operator && !keyword.is_empty() => {
                format!("{tag} {keyword}")
            }
            Some(tag) if !has_language_operator => tag.to_owned(),
            _ => keyword.clone(),
        }
    })
}

fn append_navigation(url: &mut Url, request: &GalleryListUrlRequest) {
    append_non_blank(url, "prev", request.previous.as_deref());
    append_non_blank(url, "next", request.next.as_deref());
    append_non_blank(url, "seek", request.seek.as_deref());
    if request.range > 0 {
        append(url, "range", &request.range.to_string());
    }
}

fn append_advanced_search(url: &mut Url, request: &GalleryListUrlRequest) {
    if request.advanced_search <= 0
        && request.minimum_rating <= 0
        && request.page_from <= 0
        && request.page_to <= 0
    {
        return;
    }
    append(url, "advsearch", "1");
    for (mask, name) in [
        (ADVANCED_SEARCH_SH, "f_sh"),
        (ADVANCED_SEARCH_STO, "f_sto"),
        (ADVANCED_SEARCH_SFL, "f_sfl"),
        (ADVANCED_SEARCH_SFU, "f_sfu"),
        (ADVANCED_SEARCH_SFT, "f_sft"),
    ] {
        if request.advanced_search & mask != 0 {
            append(url, name, "on");
        }
    }
    if request.minimum_rating > 0 {
        append(url, "f_srdd", &request.minimum_rating.to_string());
    }
    if request.page_from > 0 {
        append(url, "f_spf", &request.page_from.to_string());
    }
    if request.page_to > 0 {
        append(url, "f_spt", &request.page_to.to_string());
    }
}

fn append_non_blank(url: &mut Url, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        append(url, name, value);
    }
}

fn append(url: &mut Url, name: &str, value: &str) {
    url.query_pairs_mut().append_pair(name, value);
}

fn required<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, RustError> {
    value.ok_or_else(|| RustError::Error(message.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::RustError;
    use serde_json::json;

    use super::{
        API_VERSION, GalleryListMode, GalleryListSite, GalleryListUrlPlan, GalleryListUrlRequest,
        MODULE, build, build_envelope,
    };

    #[test]
    fn normal_mode_preserves_guest_category_search_and_parameter_order() {
        let mut request = normal_request();
        request.signed_in = false;
        request.keyword = Some("cats & dogs".to_owned());
        request.language_tag = Some("language:english".to_owned());
        request.previous = Some("  ".to_owned());
        request.next = Some("20-1".to_owned());
        request.seek = Some("2026-08-03 10:20".to_owned());
        request.range = 15;
        request.advanced_search = 1 | 2 | 4 | 8 | 16;
        request.minimum_rating = 4;
        request.page_from = 10;
        request.page_to = 200;

        assert_eq!(
            build(&request)
                .expect("a complete normal request should build")
                .url,
            "https://e-hentai.org/?f_cats=767&f_search=language%3Aenglish+cats+%26+dogs&next=20-1&seek=2026-08-03+10%3A20&range=15&advsearch=1&f_sh=on&f_sto=on&f_sfl=on&f_sfu=on&f_sft=on&f_srdd=4&f_spf=10&f_spt=200"
        );

        request.signed_in = true;
        request.category = 1;
        assert!(
            build(&request)
                .expect("a signed-in category request should build")
                .url
                .starts_with("https://e-hentai.org/?f_cats=1022&")
        );
    }

    #[test]
    fn language_prefix_matches_empty_and_explicit_operator_rules() {
        let mut request = normal_request();
        request.site = GalleryListSite::ExHentai;
        request.language_tag = Some("language:chinese".to_owned());
        request.keyword = Some(String::new());
        assert_eq!(
            build(&request)
                .expect("a language-only query should build")
                .url,
            "https://exhentai.org/?f_search=language%3Achinese"
        );

        for keyword in ["gid:12", "l:chinese", "language:chinese"] {
            request.keyword = Some(keyword.to_owned());
            assert_eq!(
                build(&request)
                    .expect("an explicit operator should build")
                    .url,
                format!(
                    "https://exhentai.org/?f_search={}",
                    keyword.replace(':', "%3A")
                )
            );
        }

        request.keyword = Some("   ".to_owned());
        request.language_tag = None;
        assert_eq!(
            build(&request)
                .expect("a blank query should be omitted")
                .url,
            "https://exhentai.org/"
        );
    }

    #[test]
    fn category_base_cases_preserve_signed_in_and_guest_rules() {
        let mut request = normal_request();
        assert_eq!(
            build(&request)
                .expect("the signed-in default should build")
                .url,
            "https://e-hentai.org/"
        );

        request.category = 0;
        assert_eq!(
            build(&request)
                .expect("a signed-in zero category should build")
                .url,
            "https://e-hentai.org/"
        );

        request.signed_in = false;
        request.category = 5;
        assert_eq!(
            build(&request)
                .expect("a positive guest category should build")
                .url,
            "https://e-hentai.org/?f_cats=1018"
        );
    }

    #[test]
    fn named_catalog_navigation_preserves_all_fields_and_order() {
        let mut request = normal_request();
        request.mode = GalleryListMode::Uploader;
        request.keyword = Some("alice".to_owned());
        request.previous = Some("10 & old".to_owned());
        request.next = Some("20".to_owned());
        request.seek = Some("2026-08-03".to_owned());
        request.range = 8;
        assert_eq!(
            build(&request)
                .expect("complete uploader navigation should build")
                .url,
            "https://e-hentai.org/uploader/alice?prev=10+%26+old&next=20&seek=2026-08-03&range=8"
        );

        request.mode = GalleryListMode::Tag;
        request.keyword = Some("female:full color".to_owned());
        request.previous = Some("\t".to_owned());
        request.next = Some(String::new());
        request.seek = Some(" ".to_owned());
        request.range = -1;
        assert_eq!(
            build(&request)
                .expect("blank navigation fields should be omitted")
                .url,
            "https://e-hentai.org/tag/female:full%20color"
        );
    }

    #[test]
    fn query_values_use_form_encoding_without_changing_field_order() {
        let mut request = normal_request();
        request.keyword = Some("%#?+ / 日本語".to_owned());
        request.previous = Some("before+one".to_owned());
        assert_eq!(
            build(&request)
                .expect("special search characters should encode")
                .url,
            "https://e-hentai.org/?f_search=%25%23%3F%2B+%2F+%E6%97%A5%E6%9C%AC%E8%AA%9E&prev=before%2Bone"
        );

        request.mode = GalleryListMode::ImageSearch;
        request.image_hash = Some("ab+/=%?# 日本語".to_owned());
        assert_eq!(
            build(&request)
                .expect("special hash characters should encode")
                .url,
            "https://e-hentai.org/?f_shash=ab%2B%2F%3D%25%3F%23+%E6%97%A5%E6%9C%AC%E8%AA%9E"
        );
    }

    #[test]
    fn special_modes_preserve_site_path_encoding_and_toplist_host() {
        let mut request = normal_request();
        request.site = GalleryListSite::ExHentai;
        request.mode = GalleryListMode::Subscription;
        request.category = 0;
        assert_eq!(
            build(&request)
                .expect("a subscription URL should build")
                .url,
            "https://exhentai.org/watched"
        );

        request.mode = GalleryListMode::Uploader;
        request.keyword = Some("artist/name + 日本語".to_owned());
        request.next = Some("30".to_owned());
        assert_eq!(
            build(&request).expect("an uploader URL should build").url,
            "https://exhentai.org/uploader/artist%2Fname%20+%20%E6%97%A5%E6%9C%AC%E8%AA%9E?next=30"
        );

        request.mode = GalleryListMode::Tag;
        request.keyword = Some(String::new());
        request.next = None;
        assert_eq!(
            build(&request)
                .expect("an empty tag segment is legacy-valid")
                .url,
            "https://exhentai.org/tag/"
        );

        request.mode = GalleryListMode::WhatsHot;
        assert_eq!(
            build(&request).expect("a popular URL should build").url,
            "https://exhentai.org/popular"
        );

        request.mode = GalleryListMode::ImageSearch;
        request.image_hash = Some(String::new());
        assert_eq!(
            build(&request)
                .expect("an empty image hash is legacy-valid")
                .url,
            "https://exhentai.org/?f_shash="
        );

        request.mode = GalleryListMode::Toplist;
        request.keyword = Some("15".to_owned());
        request.seek = Some("2".to_owned());
        assert_eq!(
            build(&request).expect("a toplist URL should build").url,
            "https://e-hentai.org/toplist.php?tl=15&p=2"
        );
    }

    #[test]
    fn required_values_and_protocol_version_return_sanitized_errors() {
        let mut request = normal_request();
        request.mode = GalleryListMode::Uploader;
        assert_eq!(
            build(&request),
            Err(RustError::Error(
                "gallery-list URL keyword is missing".to_owned()
            ))
        );
        request.mode = GalleryListMode::Tag;
        assert_eq!(
            build(&request),
            Err(RustError::Error(
                "gallery-list URL keyword is missing".to_owned()
            ))
        );
        request.mode = GalleryListMode::Toplist;
        assert_eq!(
            build(&request),
            Err(RustError::Error(
                "gallery-list toplist value is missing".to_owned()
            ))
        );
        request.mode = GalleryListMode::ImageSearch;
        assert_eq!(
            build(&request),
            Err(RustError::Error(
                "gallery-list image-search hash is missing".to_owned()
            ))
        );

        let missing_hash = build_envelope(&request);
        assert_eq!(
            missing_hash.error,
            Some(RustError::Error(
                "gallery-list image-search hash is missing".to_owned()
            ))
        );

        request.version = 2;
        let envelope = build_envelope(&request);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(
                "unsupported gallery-list URL protocol version".to_owned()
            ))
        );
    }

    #[test]
    fn request_and_plan_have_stable_json_and_cbor_contracts() {
        let mut request = normal_request();
        request.site = GalleryListSite::ExHentai;
        request.mode = GalleryListMode::Tag;
        request.keyword = Some("artist:alice".to_owned());
        let expected_json = json!({
            "version": 1,
            "site": "exHentai",
            "signedIn": true,
            "mode": "tag",
            "previous": null,
            "next": null,
            "seek": null,
            "range": 0,
            "category": -1,
            "keyword": "artist:alice",
            "imageHash": null,
            "languageTag": null,
            "advancedSearch": -1,
            "minimumRating": -1,
            "pageFrom": -1,
            "pageTo": -1,
        });
        assert_eq!(
            serde_json::to_value(&request).expect("request JSON should encode"),
            expected_json
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&request).expect("V1 request should encode")),
            "b06776657273696f6e01647369746568657848656e746169687369676e6564496ef5646d6f6465637461676870726576696f7573f6646e657874f6647365656bf66572616e6765006863617465676f727920676b6579776f72646c6172746973743a616c69636569696d61676548617368f66b6c616e6775616765546167f66e616476616e636564536561726368206d6d696e696d756d526174696e6720687061676546726f6d206670616765546f20"
        );

        let envelope = build_envelope(&request);
        assert_eq!(
            envelope.payload,
            Some(GalleryListUrlPlan {
                url: "https://exhentai.org/tag/artist:alice".to_owned(),
            })
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should encode");
        assert_eq!(
            to_hex(&encoded),
            "a46776657273696f6e01666d6f64756c656e67616c6c6572794c69737455726c677061796c6f6164a16375726c782568747470733a2f2f657868656e7461692e6f72672f7461672f6172746973743a616c696365656572726f72f6"
        );
    }

    #[test]
    fn every_site_and_mode_has_a_stable_wire_discriminant() {
        assert_eq!(
            serde_json::to_value(GalleryListSite::Public).expect("site should encode"),
            json!("public")
        );
        assert_eq!(
            serde_json::to_value(GalleryListSite::ExHentai).expect("site should encode"),
            json!("exHentai")
        );
        let modes = [
            (GalleryListMode::Normal, "normal"),
            (GalleryListMode::Uploader, "uploader"),
            (GalleryListMode::Tag, "tag"),
            (GalleryListMode::WhatsHot, "whatsHot"),
            (GalleryListMode::ImageSearch, "imageSearch"),
            (GalleryListMode::Subscription, "subscription"),
            (GalleryListMode::Toplist, "toplist"),
        ];
        for (mode, expected) in modes {
            assert_eq!(
                serde_json::to_value(mode).expect("mode should encode"),
                json!(expected)
            );
        }
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        hex
    }

    fn normal_request() -> GalleryListUrlRequest {
        GalleryListUrlRequest {
            version: API_VERSION,
            site: GalleryListSite::Public,
            signed_in: true,
            mode: GalleryListMode::Normal,
            previous: None,
            next: None,
            seek: None,
            range: 0,
            category: -1,
            keyword: None,
            image_hash: None,
            language_tag: None,
            advanced_search: -1,
            minimum_rating: -1,
            page_from: -1,
            page_to: -1,
        }
    }
}
