//! Portable comment/API request shapes and complete gallery/navigation URL construction.
//! Android supplies the site snapshot, attaches credentials, and performs HTTP.

use super::{FormField, GallerySite};
use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use serde_json::{Value, json};
use std::fmt::Write as _;

/// Request planner protocol version.
pub const API_VERSION: u8 = 1;
/// Stable request-planning envelope discriminator.
pub const MODULE: &str = "galleryRequest";
/// Combined UTF-8 token and tracker key budget for encoded navigation kinds.
pub const NAVIGATION_TEXT_BUDGET: usize = 64 * 1024;
/// Wiki raw UTF-16LE byte budget, retaining up to 65,536 Java code units.
pub const WIKI_UTF16_BYTE_BUDGET: usize = 2 * NAVIGATION_TEXT_BUDGET;
const WIKI_PREFIX: &str = "https://ehwiki.org/wiki/";

/// Selected complete request operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    /// Comment creation/editing form.
    Comment,
    /// Reader page API.
    ShowPage,
    /// Gallery rating API.
    Rate,
    /// Gallery metadata API.
    Metadata,
    /// Comment voting API.
    VoteComment,
    /// Tag voting API.
    VoteTag,
    /// Resolve a gallery token from a page token.
    Token,
    /// Gallery Detail navigation URL.
    DetailUrl,
    /// Reader page navigation URL.
    PageUrl,
    /// Multi-page-viewer navigation URL.
    MpvUrl,
    /// Add-favorites popup URL.
    AddFavoritesUrl,
    /// Torrent-list URL.
    TorrentUrl,
    /// Tracker announce URL; missing and empty keys are distinct.
    TrackerUrl,
    /// Wiki tag definition; only ASCII spaces become underscores.
    TagDefinitionUrl,
}

/// Ordered gallery identity in one metadata batch.
#[derive(Debug, Deserialize, Serialize)]
pub struct RequestIdentity {
    /// Signed identifier, retained unchanged.
    pub gid: i64,
    /// Opaque public gallery token; not an API key or cookie.
    pub token: String,
}

/// V1 pure request input, deliberately excluding apiuid/apikey and cookies.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryRequest {
    /// Protocol version.
    pub version: u8,
    /// Site captured once before planning.
    pub site: GallerySite,
    /// Whole operation selected before HTTP.
    pub kind: RequestKind,
    /// Gallery identifier.
    #[serde(default, skip_serializing_if = "is_default")]
    pub gid: i64,
    /// Gallery/page token.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    /// Zero-based page number or Detail list index.
    #[serde(default, skip_serializing_if = "is_default")]
    pub index: i32,
    /// Whether Detail requests all comments.
    #[serde(default, skip_serializing_if = "is_default")]
    pub all_comments: bool,
    /// Optional original-image reload parameter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nl: Option<String>,
    /// Existing comment URL supplied by Android.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Exact submitted comment text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Comment identifier; missing or -1 creates a new comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<i64>,
    /// Single-precision submitted rating.
    #[serde(default, skip_serializing_if = "is_default")]
    pub rating: f32,
    /// Optional reader show key; null remains JSON null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_key: Option<String>,
    /// Previous page token used only for positive reader indices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_token: Option<String>,
    /// Source-ordered metadata batch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<RequestIdentity>,
    /// Submitted comment/tag vote, unchanged.
    #[serde(default, skip_serializing_if = "is_default")]
    pub vote: i32,
    /// Exact submitted tag string.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tags: String,
    /// Optional tracker path key; never an API credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Raw wiki tag UTF-16LE bytes, including unpaired Java surrogate units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_tag_utf16: Option<ByteBuf>,
}

/// Complete credential-free plan; JSON field order matches the removed Kotlin builder.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryRequestPlan {
    /// Echoed operation for Android contract validation.
    pub kind: RequestKind,
    /// Request/navigation target.
    pub url: String,
    /// Optional Referer.
    pub referer: Option<String>,
    /// Optional Origin.
    pub origin: Option<String>,
    /// Ordered comment form fields.
    pub form: Vec<FormField>,
    /// JSON body without API credentials.
    pub body: Option<String>,
    /// Android inserts apiuid/apikey immediately after method when true.
    pub credentials: bool,
    /// Wiki-only raw UTF-16LE target; the text URL slot is empty for this kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_url_utf16: Option<ByteBuf>,
    /// Reference to the sole wiki source slot, always zero; absent for all other kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wiki_source_index: Option<u8>,
}

/// Builds one complete immutable plan or a fixed, input-free typed failure.
#[must_use]
pub fn build_envelope(request: &GalleryRequest) -> ApiEnvelope<GalleryRequestPlan> {
    if request.version != API_VERSION {
        return error("Unsupported gallery request version");
    }
    if request.identities.len() > 250 {
        return error("Gallery request exceeds batch budget");
    }
    if matches!(
        request.kind,
        RequestKind::AddFavoritesUrl | RequestKind::TorrentUrl | RequestKind::TrackerUrl
    ) && request.token.len() + request.key.as_ref().map_or(0, String::len)
        > NAVIGATION_TEXT_BUDGET
    {
        return error("Gallery navigation exceeds text budget");
    }
    if request.kind == RequestKind::TagDefinitionUrl {
        let Some(tag) = &request.wiki_tag_utf16 else {
            return error("Invalid gallery wiki request shape");
        };
        if tag.len() > WIKI_UTF16_BYTE_BUDGET {
            return error("Gallery wiki exceeds raw byte budget");
        }
        if tag.len() % 2 != 0 || !request.token.is_empty() || request.key.is_some() {
            return error("Invalid gallery wiki request shape");
        }
    } else if request.wiki_tag_utf16.is_some() {
        return error("Invalid gallery wiki request shape");
    }
    match plan(request) {
        Some(plan) => ApiEnvelope::ok(API_VERSION, MODULE, plan),
        None => error("Invalid gallery request shape"),
    }
}
fn error(message: &str) -> ApiEnvelope<GalleryRequestPlan> {
    ApiEnvelope::err(API_VERSION, MODULE, RustError::Error(message.to_owned()))
}

fn base_plan(request: &GalleryRequest, origin: &str) -> GalleryRequestPlan {
    GalleryRequestPlan {
        kind: request.kind,
        url: match request.site {
            GallerySite::Public => "https://api.e-hentai.org/api.php",
            GallerySite::ExHentai => "https://s.exhentai.org/api.php",
        }
        .to_owned(),
        referer: Some(origin.to_owned()),
        origin: Some(origin.to_owned()),
        form: Vec::new(),
        body: None,
        credentials: false,
        wiki_url_utf16: None,
        wiki_source_index: None,
    }
}
fn plan(request: &GalleryRequest) -> Option<GalleryRequestPlan> {
    let origin = format!("https://{}", request.site.host());
    let mut plan = base_plan(request, &origin);
    let text = |value: &str| Value::String(value.to_owned());
    let gid = json!(request.gid);
    let token = text(&request.token);
    match request.kind {
        RequestKind::Comment => {
            let url = request.url.as_ref()?;
            let comment = request.comment.as_ref()?;
            plan.url.clone_from(url);
            plan.referer = Some(url.clone());
            plan.form = comment_fields(request, comment);
        }
        RequestKind::ShowPage => {
            plan.referer = if request.index > 0 {
                request.previous_token.as_ref().map(|previous| {
                    page_url(&origin, request.gid, request.index - 1, previous, None)
                })
            } else {
                None
            };
            plan.body = Some(body(vec![
                ("method", text("showpage")),
                ("gid", gid),
                ("page", json!(request.index.wrapping_add(1))),
                ("imgkey", token),
                (
                    "showkey",
                    request
                        .show_key
                        .as_ref()
                        .map_or(Value::Null, |key| text(key)),
                ),
            ]));
        }
        RequestKind::Rate => {
            plan.referer = Some(detail_url(&origin, request.gid, &request.token, 0, false));
            plan.credentials = true;
            plan.body = Some(body(vec![
                ("method", text("rategallery")),
                ("gid", gid),
                ("token", token),
                ("rating", json!(rating_steps(request.rating))),
            ]));
        }
        RequestKind::Metadata => {
            plan.referer.clone_from(&request.url);
            plan.body = Some(metadata_body(request));
        }
        RequestKind::VoteComment => {
            plan.credentials = true;
            plan.body = Some(body(vec![
                ("method", text("votecomment")),
                ("gid", gid),
                ("token", token),
                ("comment_id", json!(request.comment_id?)),
                ("comment_vote", json!(request.vote)),
            ]));
        }
        RequestKind::VoteTag => {
            plan.credentials = true;
            plan.body = Some(body(vec![
                ("method", text("taggallery")),
                ("gid", gid),
                ("token", token),
                ("tags", text(&request.tags)),
                ("vote", json!(request.vote)),
            ]));
        }
        RequestKind::Token => {
            plan.body = Some(body(vec![
                ("method", text("gtoken")),
                (
                    "pagelist",
                    json!([[request.gid, request.token, request.index.wrapping_add(1)]]),
                ),
            ]));
        }
        RequestKind::DetailUrl
        | RequestKind::PageUrl
        | RequestKind::MpvUrl
        | RequestKind::AddFavoritesUrl
        | RequestKind::TorrentUrl
        | RequestKind::TrackerUrl
        | RequestKind::TagDefinitionUrl => return navigation_plan(plan, request, &origin),
    }
    Some(plan)
}
fn navigation_plan(
    mut plan: GalleryRequestPlan,
    request: &GalleryRequest,
    origin: &str,
) -> Option<GalleryRequestPlan> {
    if request.kind == RequestKind::TagDefinitionUrl {
        plan.url.clear();
        plan.wiki_url_utf16 = Some(wiki_url_utf16(request.wiki_tag_utf16.as_ref()?));
        plan.wiki_source_index = Some(0);
    } else {
        plan.url = navigation_url(request, origin);
    }
    plan.referer = None;
    plan.origin = None;
    Some(plan)
}
fn wiki_url_utf16(tag: &[u8]) -> ByteBuf {
    let mut result = Vec::with_capacity(WIKI_PREFIX.len() * 2 + tag.len());
    for byte in WIKI_PREFIX.bytes() {
        result.extend_from_slice(&[byte, 0]);
    }
    for unit in tag.chunks_exact(2) {
        if unit == [b' ', 0] {
            result.extend_from_slice(&[b'_', 0]);
        } else {
            result.extend_from_slice(unit);
        }
    }
    ByteBuf::from(result)
}
fn metadata_body(request: &GalleryRequest) -> String {
    let rows: Vec<Value> = request
        .identities
        .iter()
        .map(|identity| json!([identity.gid, identity.token]))
        .collect();
    body(vec![
        ("method", Value::String("gdata".to_owned())),
        ("gidlist", json!(rows)),
        ("namespace", json!(1)),
    ])
}
fn navigation_url(request: &GalleryRequest, origin: &str) -> String {
    match request.kind {
        RequestKind::DetailUrl => detail_url(
            origin,
            request.gid,
            &request.token,
            request.index,
            request.all_comments,
        ),
        RequestKind::PageUrl => page_url(
            origin,
            request.gid,
            request.index,
            &request.token,
            request.nl.as_deref(),
        ),
        RequestKind::AddFavoritesUrl | RequestKind::TorrentUrl => {
            let script = if request.kind == RequestKind::AddFavoritesUrl {
                "gallerypopups.php"
            } else {
                "gallerytorrents.php"
            };
            let mut url = format!(
                "{origin}/{script}?gid={}&t={}",
                request.gid,
                encode(&request.token, false)
            );
            if request.kind == RequestKind::AddFavoritesUrl {
                url.push_str("&act=addfav");
            }
            url
        }
        RequestKind::TrackerUrl => {
            let mut url = format!("https://ehtracker.org/{}/", request.gid);
            if let Some(key) = &request.key {
                url.push_str(&encode(key, true));
                url.push('/');
            }
            url.push_str("announce");
            url
        }
        _ => format!(
            "{origin}/mpv/{}/{}",
            request.gid,
            encode(&request.token, true)
        ),
    }
}
fn comment_fields(request: &GalleryRequest, comment: &str) -> Vec<FormField> {
    let id = request.comment_id.unwrap_or(-1);
    if id == -1 {
        vec![field("commenttext_new", comment)]
    } else {
        vec![
            field("commenttext_edit", comment),
            field("edit_comment", id.to_string()),
        ]
    }
}
fn field(name: &str, value: impl Into<String>) -> FormField {
    FormField {
        name: name.to_owned(),
        value: value.into(),
    }
}
fn body(fields: Vec<(&str, Value)>) -> String {
    let mut result = String::from("{");
    for (index, (name, value)) in fields.into_iter().enumerate() {
        if index != 0 {
            result.push(',');
        }
        result.push_str(&serde_json::to_string(name).expect("string serialization"));
        result.push(':');
        result.push_str(&serde_json::to_string(&value).expect("finite JSON serialization"));
    }
    result.push('}');
    result
}
#[allow(
    clippy::cast_possible_truncation,
    reason = "Rust float-to-int saturation matches Kotlin Double.toInt, including NaN/infinity"
)]
fn rating_steps(rating: f32) -> i32 {
    f64::from(rating * 2.0).ceil() as i32
}
fn detail_url(origin: &str, gid: i64, token: &str, index: i32, comments: bool) -> String {
    let mut result = format!("{origin}/g/{gid}/{}", encode(token, true));
    if index != 0 {
        write!(result, "?p={index}").expect("String write");
    }
    if comments {
        result.push_str(if index == 0 { "?hc=1" } else { "&hc=1" });
    }
    result
}
fn page_url(origin: &str, gid: i64, index: i32, token: &str, nl: Option<&str>) -> String {
    let mut result = format!(
        "{origin}/s/{}/{gid}-{}",
        encode(token, true),
        index.wrapping_add(1)
    );
    if let Some(nl) = nl.filter(|value| !value.chars().all(kotlin_whitespace)) {
        result.push_str("?nl=");
        result.push_str(&encode(nl, false));
    }
    result
}
// Matches Ktor encodeURLPathPart and encodeURLParameterValue, not form_urlencoded's '*'/ '~' set.
fn encode(value: &str, path: bool) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        let allowed = byte.is_ascii_alphanumeric()
            || if path {
                b":@!$&'()*+,;=-._~".contains(&byte)
            } else {
                b"-._~".contains(&byte)
            };
        if allowed {
            result.push(char::from(byte));
        } else if !path && byte == b' ' {
            result.push('+');
        } else {
            write!(result, "%{byte:02X}").expect("String write");
        }
    }
    result
}
fn kotlin_whitespace(value: char) -> bool {
    (value.is_whitespace() && value != '\u{0085}') || matches!(value, '\u{001c}'..='\u{001f}')
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    value == &T::default()
}

#[cfg(test)]
mod tests;
