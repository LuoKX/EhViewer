//! Pure favorites-list URL construction.
//!
//! Android retains the mutable, serializable favorites navigation state and
//! local-favorites database path. This module owns only the remote Favorites URL
//! shape: selected gallery host, category mapping, ordered query parameters, and
//! percent-encoding. Cookies, credentials, gallery tokens, response bodies, and
//! Android objects never enter this protocol.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use url::Url;

const FAVORITES_PATH: &str = "favorites.php";
const FAVORITE_CATEGORY_ALL: i32 = -1;

/// Current favorites-list URL bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable favorites-list URL module discriminator used by Android adapters.
pub const MODULE: &str = "galleryFavoritesUrl";

/// Gallery site selected by Android settings.
/// Backwards-compatible source name for the shared gallery site.
pub use super::GallerySite as FavoritesSite;

/// Complete sanitized state needed to construct one remote favorites-list URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesUrlRequest {
    /// Protocol version; callers must use [`API_VERSION`].
    pub version: u8,
    /// Gallery host selected by Android.
    pub site: FavoritesSite,
    /// Legacy favorite category: `-1` means all, `0..=9` means a remote slot,
    /// and every other value omits the `favcat` parameter.
    pub favorite_category: i32,
    /// Optional favorites search text, omitted when blank.
    pub keyword: Option<String>,
    /// Previous-page cursor, omitted when blank.
    pub previous: Option<String>,
    /// Next-page cursor, omitted when blank.
    pub next: Option<String>,
    /// Date cursor, omitted when blank.
    pub seek: Option<String>,
}

/// Versioned output payload containing one fully encoded Favorites request target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesUrlPlan {
    /// HTTPS URL with legacy-compatible path and ordered query parameters.
    pub url: String,
}

/// Builds a versioned success/error envelope for the Android bridge.
#[must_use]
pub fn build_envelope(request: &FavoritesUrlRequest) -> ApiEnvelope<FavoritesUrlPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error("unsupported favorites URL protocol version".to_owned()),
        );
    }
    ApiEnvelope::ok(API_VERSION, MODULE, build(request))
}

/// Constructs one remote favorites-list URL without platform or session access.
#[must_use]
pub fn build(request: &FavoritesUrlRequest) -> FavoritesUrlPlan {
    let mut url = base_url(request.site.host());
    if (0..=9).contains(&request.favorite_category) {
        append(&mut url, "favcat", &request.favorite_category.to_string());
    } else if request.favorite_category == FAVORITE_CATEGORY_ALL {
        append(&mut url, "favcat", "all");
    }
    append_non_blank(&mut url, "f_search", request.keyword.as_deref());
    append_non_blank(&mut url, "prev", request.previous.as_deref());
    append_non_blank(&mut url, "next", request.next.as_deref());
    append_non_blank(&mut url, "seek", request.seek.as_deref());
    FavoritesUrlPlan { url: url.into() }
}

fn base_url(host: &str) -> Url {
    Url::parse(&format!("https://{host}/{FAVORITES_PATH}"))
        .expect("source-controlled Favorites hosts and path must form a valid HTTPS URL")
}

fn append_non_blank(url: &mut Url, name: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
        append(url, name, value);
    }
}

fn append(url: &mut Url, name: &str, value: &str) {
    url.query_pairs_mut().append_pair(name, value);
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::RustError;
    use serde_json::json;

    use super::{
        API_VERSION, FavoritesSite, FavoritesUrlPlan, FavoritesUrlRequest, MODULE, build,
        build_envelope,
    };

    #[test]
    fn maps_all_remote_and_non_remote_category_values_exactly() {
        let mut request = request();
        assert_eq!(
            build(&request).url,
            "https://e-hentai.org/favorites.php?favcat=all"
        );

        request.site = FavoritesSite::ExHentai;
        request.favorite_category = 3;
        assert_eq!(
            build(&request).url,
            "https://exhentai.org/favorites.php?favcat=3"
        );

        for category in [-3, -2, 10, i32::MAX] {
            request.favorite_category = category;
            assert_eq!(build(&request).url, "https://exhentai.org/favorites.php");
        }
    }

    #[test]
    fn preserves_complete_query_order_and_blank_omission() {
        let mut request = request();
        request.favorite_category = 9;
        request.keyword = Some("cats & dogs".to_owned());
        request.previous = Some("10 & old".to_owned());
        request.next = Some("20".to_owned());
        request.seek = Some("2026-08-03 10:20".to_owned());
        assert_eq!(
            build(&request).url,
            "https://e-hentai.org/favorites.php?favcat=9&f_search=cats+%26+dogs&prev=10+%26+old&next=20&seek=2026-08-03+10%3A20"
        );

        request.keyword = Some("  ".to_owned());
        request.previous = Some("\t".to_owned());
        request.next = Some(String::new());
        request.seek = None;
        assert_eq!(
            build(&request).url,
            "https://e-hentai.org/favorites.php?favcat=9"
        );
    }

    #[test]
    fn form_encodes_special_and_unicode_query_values() {
        let mut request = request();
        request.favorite_category = -2;
        request.keyword = Some("%#?+ / 日本語".to_owned());
        request.previous = Some("before+one".to_owned());
        assert_eq!(
            build(&request).url,
            "https://e-hentai.org/favorites.php?f_search=%25%23%3F%2B+%2F+%E6%97%A5%E6%9C%AC%E8%AA%9E&prev=before%2Bone"
        );
    }

    #[test]
    fn request_and_response_have_stable_json_and_cbor_contracts() {
        let request = FavoritesUrlRequest {
            version: API_VERSION,
            site: FavoritesSite::ExHentai,
            favorite_category: 4,
            keyword: Some("translated".to_owned()),
            previous: None,
            next: Some("20".to_owned()),
            seek: None,
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request JSON should encode"),
            json!({
                "version": 1,
                "site": "exHentai",
                "favoriteCategory": 4,
                "keyword": "translated",
                "previous": null,
                "next": "20",
                "seek": null,
            })
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&request).expect("V1 request should encode")),
            "a76776657273696f6e01647369746568657848656e746169706661766f7269746543617465676f727904676b6579776f72646a7472616e736c617465646870726576696f7573f6646e657874623230647365656bf6"
        );

        let envelope = build_envelope(&request);
        assert_eq!(
            envelope.payload,
            Some(FavoritesUrlPlan {
                url: "https://exhentai.org/favorites.php?favcat=4&f_search=translated&next=20"
                    .to_owned(),
            })
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974657355726c677061796c6f6164a16375726c784768747470733a2f2f657868656e7461692e6f72672f6661766f72697465732e7068703f6661766361743d3426665f7365617263683d7472616e736c61746564266e6578743d3230656572726f72f6"
        );
    }

    #[test]
    fn rejects_unsupported_versions_without_echoing_request_fields() {
        let mut request = request();
        request.version = 2;
        request.keyword = Some("private search".to_owned());
        let envelope = build_envelope(&request);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(
                "unsupported favorites URL protocol version".to_owned()
            ))
        );
        assert_eq!(
            serde_json::to_value(FavoritesSite::Public).expect("site should encode"),
            json!("public")
        );
        assert_eq!(
            serde_json::to_value(FavoritesSite::ExHentai).expect("site should encode"),
            json!("exHentai")
        );
    }

    fn request() -> FavoritesUrlRequest {
        FavoritesUrlRequest {
            version: API_VERSION,
            site: FavoritesSite::Public,
            favorite_category: -1,
            keyword: None,
            previous: None,
            next: None,
            seek: None,
        }
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
        }
        hex
    }
}
