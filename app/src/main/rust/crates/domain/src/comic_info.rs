//! Pure `ComicInfo` projection and XML compatibility contracts.
//!
//! Android owns paths and gallery URLs. This module owns gallery-tag projection,
//! simple-tag projection, stable rating formatting, and bounded `ComicInfo` XML.

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use crate::contracts::{ApiEnvelope, RustError};

/// Version of the `ComicInfo` CBOR contract.
pub const COMIC_INFO_API_VERSION: u8 = 1;
/// Stable module identifier for `ComicInfo` envelopes.
pub const COMIC_INFO_MODULE: &str = "comicInfo";
/// Maximum accepted or produced `ComicInfo` XML length.
pub const MAX_COMIC_INFO_BYTES: usize = 512 * 1024;
/// `ComicInfo` 2.0 XML Schema instance namespace.
pub const XML_SCHEMA_INSTANCE: &str = "http://www.w3.org/2001/XMLSchema-instance";
/// `ComicInfo` 2.0 schema location emitted by the legacy Android writer.
pub const XML_SCHEMA_LOCATION: &str =
    "https://raw.githubusercontent.com/anansi-project/comicinfo/main/schema/v2.0/ComicInfo.xsd";

/// Portable `ComicInfo` fields used by archive metadata and local recovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfo {
    /// Primary gallery title.
    pub series: Option<String>,
    /// Alternate gallery title.
    pub alternate_series: Option<String>,
    /// Group tags.
    pub writer: Option<Vec<String>>,
    /// Artist and cosplayer tags.
    pub penciller: Option<Vec<String>>,
    /// Gendered, location, and other tags.
    pub genre: Option<Vec<String>>,
    /// Android-constructed gallery URL.
    pub web: Option<String>,
    /// Declared image-page count.
    #[serde(default)]
    pub page_count: i32,
    /// Lowercase simple language label.
    pub language_iso: Option<String>,
    /// Character tags.
    pub characters: Option<Vec<String>>,
    /// Parody tags.
    pub teams: Option<Vec<String>>,
    /// Locale-independent one-decimal rating text.
    pub community_rating: Option<String>,
    /// Flattened legacy tags in writer-to-team field order.
    #[serde(default)]
    pub simple_tags: Option<Vec<String>>,
}

/// Strength of one structured gallery tag.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComicInfoTagPower {
    /// Normal-strength tag.
    Solid,
    /// Positively reinforced tag.
    Active,
    /// Weak tag omitted by detailed-gallery projection.
    Weak,
}

/// One structured gallery tag projected across the Android boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoTag {
    /// Source tag text.
    pub text: String,
    /// Source tag strength.
    pub power: ComicInfoTagPower,
}

/// One source namespace and its structured tags.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoTagGroup {
    /// Stable source namespace value.
    pub namespace: String,
    /// Tags in source order.
    pub tags: Vec<ComicInfoTag>,
}

/// Versioned request for projecting gallery metadata into `ComicInfo`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryComicInfoRequest {
    /// Requested contract version.
    pub version: u8,
    /// Primary gallery title.
    pub title: Option<String>,
    /// Optional alternate title.
    pub title_jpn: Option<String>,
    /// Android-constructed gallery URL.
    pub web: String,
    /// Source rating using existing Android `Float` precision.
    pub rating: f32,
    /// Source page count.
    pub pages: i32,
    /// Optional derived language label.
    pub simple_language: Option<String>,
    /// Listing-style namespace-prefixed tags.
    pub simple_tags: Option<Vec<String>>,
    /// Detailed structured tags. Presence, including an empty list, selects detail semantics.
    pub tag_groups: Option<Vec<ComicInfoTagGroup>>,
}

/// Versioned request for encoding one `ComicInfo` value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoEncodeRequest {
    /// Requested contract version.
    pub version: u8,
    /// Value to encode.
    pub info: ComicInfo,
}

/// Versioned request for decoding bounded `ComicInfo` XML bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoDecodeRequest {
    /// Requested contract version.
    pub version: u8,
    /// XML bytes encoded as a CBOR byte string.
    #[serde(with = "serde_bytes")]
    pub xml: Vec<u8>,
}

/// Bounded XML bytes returned by the `ComicInfo` encoder.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoXml {
    /// UTF-8 `ComicInfo` XML encoded as a CBOR byte string.
    pub xml: ByteBuf,
}

/// Builds a versioned gallery-to-`ComicInfo` response.
#[must_use]
pub fn build_gallery_envelope(request: GalleryComicInfoRequest) -> ApiEnvelope<ComicInfo> {
    if request.version != COMIC_INFO_API_VERSION {
        return unsupported_version();
    }
    ApiEnvelope::ok(
        COMIC_INFO_API_VERSION,
        COMIC_INFO_MODULE,
        from_gallery(request),
    )
}

/// Builds a versioned `ComicInfo` XML encode response.
#[must_use]
pub fn encode_xml_envelope(request: ComicInfoEncodeRequest) -> ApiEnvelope<ComicInfoXml> {
    let ComicInfoEncodeRequest { version, info } = request;
    if version != COMIC_INFO_API_VERSION {
        return unsupported_version();
    }
    match encode_xml(&info) {
        Ok(xml) => ApiEnvelope::ok(
            COMIC_INFO_API_VERSION,
            COMIC_INFO_MODULE,
            ComicInfoXml {
                xml: ByteBuf::from(xml),
            },
        ),
        Err(error) => ApiEnvelope::err(COMIC_INFO_API_VERSION, COMIC_INFO_MODULE, error),
    }
}

/// Builds a versioned `ComicInfo` XML decode response.
#[must_use]
pub fn decode_xml_envelope(request: ComicInfoDecodeRequest) -> ApiEnvelope<ComicInfo> {
    let ComicInfoDecodeRequest { version, xml } = request;
    if version != COMIC_INFO_API_VERSION {
        return unsupported_version();
    }
    match decode_xml(&xml) {
        Ok(info) => ApiEnvelope::ok(COMIC_INFO_API_VERSION, COMIC_INFO_MODULE, info),
        Err(error) => ApiEnvelope::err(COMIC_INFO_API_VERSION, COMIC_INFO_MODULE, error),
    }
}

/// Projects gallery fields and either structured or simple tags into `ComicInfo`.
#[must_use]
pub fn from_gallery(request: GalleryComicInfoRequest) -> ComicInfo {
    let mut artists = Vec::new();
    let mut groups = Vec::new();
    let mut characters = Vec::new();
    let mut parodies = Vec::new();
    let mut other_tags = Vec::new();

    if let Some(tag_groups) = request.tag_groups {
        for group in tag_groups {
            for tag in group
                .tags
                .into_iter()
                .filter(|tag| tag.text != "original" && tag.power != ComicInfoTagPower::Weak)
            {
                project_tag(
                    &group.namespace,
                    tag.text,
                    &mut artists,
                    &mut groups,
                    &mut characters,
                    &mut parodies,
                    &mut other_tags,
                );
            }
        }
    } else if let Some(simple_tags) = request.simple_tags {
        for value in simple_tags {
            let Some((namespace, tag)) = value.split_once(':') else {
                continue;
            };
            if namespace == "parody" && tag == "original" {
                continue;
            }
            project_tag(
                namespace,
                tag.to_owned(),
                &mut artists,
                &mut groups,
                &mut characters,
                &mut parodies,
                &mut other_tags,
            );
        }
    }

    let mut info = ComicInfo {
        series: request.title,
        alternate_series: request.title_jpn.filter(|title| !title.trim().is_empty()),
        writer: some_nonempty(groups),
        penciller: some_nonempty(artists),
        genre: some_nonempty(other_tags),
        web: Some(request.web),
        page_count: request.pages,
        language_iso: request
            .simple_language
            .map(|language| language.to_lowercase()),
        characters: some_nonempty(characters),
        teams: some_nonempty(parodies),
        community_rating: Some(format_rating(request.rating)),
        simple_tags: None,
    };
    info.simple_tags = to_simple_tags(&info);
    info
}

/// Flattens legacy simple tags in writer, penciller, genre, character, and team order.
#[must_use]
pub fn to_simple_tags(info: &ComicInfo) -> Option<Vec<String>> {
    let tags = [
        info.writer.as_ref(),
        info.penciller.as_ref(),
        info.genre.as_ref(),
        info.characters.as_ref(),
        info.teams.as_ref(),
    ]
    .into_iter()
    .flatten()
    .flatten()
    .cloned()
    .collect::<Vec<_>>();
    some_nonempty(tags)
}

/// Encodes the legacy-compatible UTF-8 `ComicInfo` XML representation.
///
/// # Errors
///
/// Returns a fixed error when a field contains an XML 1.0-invalid scalar or the
/// result exceeds the `ComicInfo` byte budget.
pub fn encode_xml(info: &ComicInfo) -> Result<Vec<u8>, RustError> {
    let mut xml = format!(
        "<?xml version='1.0' encoding='UTF-8' ?>\n<ComicInfo xmlns:xsi=\"{XML_SCHEMA_INSTANCE}\" xsi:noNamespaceSchemaLocation=\"{XML_SCHEMA_LOCATION}\">"
    );
    push_optional_text(&mut xml, "Series", info.series.as_deref())?;
    push_optional_text(
        &mut xml,
        "AlternateSeries",
        info.alternate_series.as_deref(),
    )?;
    push_optional_list(&mut xml, "Writer", info.writer.as_deref())?;
    push_optional_list(&mut xml, "Penciller", info.penciller.as_deref())?;
    push_optional_list(&mut xml, "Genre", info.genre.as_deref())?;
    push_optional_text(&mut xml, "Web", info.web.as_deref())?;
    push_text(&mut xml, "PageCount", &info.page_count.to_string())?;
    push_optional_text(&mut xml, "LanguageISO", info.language_iso.as_deref())?;
    push_optional_list(&mut xml, "Characters", info.characters.as_deref())?;
    push_optional_list(&mut xml, "Teams", info.teams.as_deref())?;
    push_optional_text(
        &mut xml,
        "CommunityRating",
        info.community_rating.as_deref(),
    )?;
    xml.push_str("\n</ComicInfo>");
    if xml.len() > MAX_COMIC_INFO_BYTES {
        return Err(invalid_xml());
    }
    Ok(xml.into_bytes())
}

/// Decodes bounded legacy-compatible `ComicInfo` XML.
///
/// Unknown direct children, including their entire nested subtrees, are ignored.
/// Missing nullable fields remain absent, missing `PageCount` defaults to zero,
/// and schema attributes are accepted but are not business data.
///
/// # Errors
///
/// Returns a fixed error for empty, oversized, malformed, or type-invalid XML.
#[allow(
    clippy::too_many_lines,
    reason = "keeping the bounded XML event state machine together makes depth transitions auditable"
)]
pub fn decode_xml(bytes: &[u8]) -> Result<ComicInfo, RustError> {
    if bytes.is_empty() || bytes.len() > MAX_COMIC_INFO_BYTES {
        return Err(invalid_xml());
    }

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut depth = 0_usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut current = None::<(ComicInfoField, String)>;
    let mut decoded = DecodedFields::default();

    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                if root_closed {
                    return Err(invalid_xml());
                }
                depth += 1;
                if depth == 1 {
                    if root_seen || start.name().as_ref() != b"ComicInfo" {
                        return Err(invalid_xml());
                    }
                    root_seen = true;
                } else if depth == 2 {
                    current = ComicInfoField::from_name(start.name().as_ref())
                        .map(|field| (field, String::new()));
                }
            }
            Ok(Event::Empty(empty)) => {
                if depth == 0 {
                    if root_seen || empty.name().as_ref() != b"ComicInfo" {
                        return Err(invalid_xml());
                    }
                    root_seen = true;
                    root_closed = true;
                } else if depth == 1
                    && let Some(field) = ComicInfoField::from_name(empty.name().as_ref())
                {
                    decoded.set(field, String::new())?;
                }
            }
            Ok(Event::Text(text)) => {
                let value = text.decode().map_err(|_| invalid_xml())?;
                let value = quick_xml::escape::unescape(&value).map_err(|_| invalid_xml())?;
                if depth == 2 {
                    if let Some((_, current_value)) = current.as_mut() {
                        current_value.push_str(&value);
                    }
                } else if (depth == 0 || root_closed) && !value.trim().is_empty() {
                    return Err(invalid_xml());
                }
            }
            Ok(Event::CData(text)) => {
                if depth == 2
                    && let Some((_, current_value)) = current.as_mut()
                {
                    current_value.push_str(&text.decode().map_err(|_| invalid_xml())?);
                }
            }
            Ok(Event::End(end)) => {
                if depth == 0 {
                    return Err(invalid_xml());
                }
                if depth == 2 {
                    if let Some((field, value)) = current.take() {
                        decoded.set(field, value)?;
                    }
                } else if depth == 1 {
                    if end.name().as_ref() != b"ComicInfo" {
                        return Err(invalid_xml());
                    }
                    root_closed = true;
                }
                depth -= 1;
            }
            Ok(Event::GeneralRef(reference)) => {
                if depth == 2
                    && let Some((_, current_value)) = current.as_mut()
                {
                    if let Some(character) =
                        reference.resolve_char_ref().map_err(|_| invalid_xml())?
                    {
                        if !is_xml_10_char(character) {
                            return Err(invalid_xml());
                        }
                        current_value.push(character);
                    } else {
                        let name = reference.decode().map_err(|_| invalid_xml())?;
                        current_value.push(match name.as_ref() {
                            "amp" => '&',
                            "lt" => '<',
                            "gt" => '>',
                            "apos" => '\'',
                            "quot" => '"',
                            _ => return Err(invalid_xml()),
                        });
                    }
                }
            }
            Ok(Event::DocType(_)) | Err(_) => return Err(invalid_xml()),
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_) | Event::PI(_) | Event::Comment(_)) => {}
        }
    }

    if !root_seen || !root_closed || depth != 0 {
        return Err(invalid_xml());
    }
    let mut info = decoded.into_comic_info();
    info.simple_tags = to_simple_tags(&info);
    Ok(info)
}

fn project_tag(
    namespace: &str,
    tag: String,
    artists: &mut Vec<String>,
    groups: &mut Vec<String>,
    characters: &mut Vec<String>,
    parodies: &mut Vec<String>,
    other_tags: &mut Vec<String>,
) {
    match namespace {
        "artist" | "cosplayer" => artists.push(tag),
        "group" => groups.push(tag),
        "character" => characters.push(tag),
        "parody" => parodies.push(tag),
        "location" | "other" => other_tags.push(tag),
        "female" => other_tags.push(format!("f:{tag}")),
        "male" => other_tags.push(format!("m:{tag}")),
        "mixed" => other_tags.push(format!("x:{tag}")),
        _ => {}
    }
}

fn format_rating(rating: f32) -> String {
    if rating.is_nan() {
        "NaN".to_owned()
    } else if rating == f32::INFINITY {
        "Infinity".to_owned()
    } else if rating == f32::NEG_INFINITY {
        "-Infinity".to_owned()
    } else {
        format!("{rating:.1}")
    }
}

fn some_nonempty(values: Vec<String>) -> Option<Vec<String>> {
    (!values.is_empty()).then_some(values)
}

fn push_optional_list(
    xml: &mut String,
    name: &str,
    values: Option<&[String]>,
) -> Result<(), RustError> {
    if let Some(values) = values {
        push_text(xml, name, &values.join(", "))?;
    }
    Ok(())
}

fn push_optional_text(xml: &mut String, name: &str, value: Option<&str>) -> Result<(), RustError> {
    if let Some(value) = value {
        push_text(xml, name, value)?;
    }
    Ok(())
}

fn push_text(xml: &mut String, name: &str, value: &str) -> Result<(), RustError> {
    if !value.chars().all(is_xml_10_char) {
        return Err(invalid_xml());
    }
    xml.push('\n');
    xml.push_str("  <");
    xml.push_str(name);
    if value.is_empty() {
        xml.push_str(" />");
        return Ok(());
    }
    xml.push('>');
    for character in value.chars() {
        match character {
            '&' => xml.push_str("&amp;"),
            '<' => xml.push_str("&lt;"),
            '>' => xml.push_str("&gt;"),
            _ => xml.push(character),
        }
    }
    xml.push_str("</");
    xml.push_str(name);
    xml.push('>');
    Ok(())
}

const fn is_xml_10_char(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{a}' | '\u{d}')
        || matches!(character as u32, 0x20..=0xd7ff | 0xe000..=0xfffd | 0x1_0000..=0x10_ffff)
}

#[derive(Clone, Copy)]
enum ComicInfoField {
    Series,
    AlternateSeries,
    Writer,
    Penciller,
    Genre,
    Web,
    PageCount,
    LanguageIso,
    Characters,
    Teams,
    CommunityRating,
}

impl ComicInfoField {
    fn from_name(name: &[u8]) -> Option<Self> {
        match name {
            b"Series" => Some(Self::Series),
            b"AlternateSeries" => Some(Self::AlternateSeries),
            b"Writer" => Some(Self::Writer),
            b"Penciller" => Some(Self::Penciller),
            b"Genre" => Some(Self::Genre),
            b"Web" => Some(Self::Web),
            b"PageCount" => Some(Self::PageCount),
            b"LanguageISO" => Some(Self::LanguageIso),
            b"Characters" => Some(Self::Characters),
            b"Teams" => Some(Self::Teams),
            b"CommunityRating" => Some(Self::CommunityRating),
            _ => None,
        }
    }
}

#[derive(Default)]
struct DecodedFields {
    series: Option<String>,
    alternate_series: Option<String>,
    writer: Option<Vec<String>>,
    penciller: Option<Vec<String>>,
    genre: Option<Vec<String>>,
    web: Option<String>,
    page_count: i32,
    language_iso: Option<String>,
    characters: Option<Vec<String>>,
    teams: Option<Vec<String>>,
    community_rating: Option<String>,
}

impl DecodedFields {
    fn set(&mut self, field: ComicInfoField, value: String) -> Result<(), RustError> {
        match field {
            ComicInfoField::Series => self.series = Some(value),
            ComicInfoField::AlternateSeries => self.alternate_series = Some(value),
            ComicInfoField::Writer => self.writer = Some(split_tags(&value)),
            ComicInfoField::Penciller => self.penciller = Some(split_tags(&value)),
            ComicInfoField::Genre => self.genre = Some(split_tags(&value)),
            ComicInfoField::Web => self.web = Some(value),
            ComicInfoField::PageCount => {
                self.page_count = value.parse::<i32>().map_err(|_| invalid_xml())?;
            }
            ComicInfoField::LanguageIso => self.language_iso = Some(value),
            ComicInfoField::Characters => self.characters = Some(split_tags(&value)),
            ComicInfoField::Teams => self.teams = Some(split_tags(&value)),
            ComicInfoField::CommunityRating => self.community_rating = Some(value),
        }
        Ok(())
    }

    fn into_comic_info(self) -> ComicInfo {
        ComicInfo {
            series: self.series,
            alternate_series: self.alternate_series,
            writer: self.writer,
            penciller: self.penciller,
            genre: self.genre,
            web: self.web,
            page_count: self.page_count,
            language_iso: self.language_iso,
            characters: self.characters,
            teams: self.teams,
            community_rating: self.community_rating,
            simple_tags: None,
        }
    }
}

fn split_tags(value: &str) -> Vec<String> {
    value.split(", ").map(ToOwned::to_owned).collect()
}

fn unsupported_version<T>() -> ApiEnvelope<T> {
    ApiEnvelope::err(
        COMIC_INFO_API_VERSION,
        COMIC_INFO_MODULE,
        RustError::Error("unsupported ComicInfo protocol version".to_owned()),
    )
}

fn invalid_xml() -> RustError {
    RustError::Error("invalid ComicInfo XML".to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn request() -> GalleryComicInfoRequest {
        GalleryComicInfoRequest {
            version: COMIC_INFO_API_VERSION,
            title: Some("Series & <title>".to_owned()),
            title_jpn: Some("   ".to_owned()),
            web: "https://e-hentai.org/g/1/0123456789/".to_owned(),
            rating: 4.5,
            pages: -1,
            simple_language: Some("EN".to_owned()),
            simple_tags: None,
            tag_groups: Some(vec![
                ComicInfoTagGroup {
                    namespace: "group".to_owned(),
                    tags: vec![
                        tag("group one", ComicInfoTagPower::Solid),
                        tag("group, two", ComicInfoTagPower::Active),
                    ],
                },
                ComicInfoTagGroup {
                    namespace: "female".to_owned(),
                    tags: vec![tag("tag", ComicInfoTagPower::Solid)],
                },
                ComicInfoTagGroup {
                    namespace: "parody".to_owned(),
                    tags: vec![
                        tag("original", ComicInfoTagPower::Solid),
                        tag("parody", ComicInfoTagPower::Active),
                    ],
                },
                ComicInfoTagGroup {
                    namespace: "artist".to_owned(),
                    tags: vec![tag("weak artist", ComicInfoTagPower::Weak)],
                },
            ]),
        }
    }

    fn tag(text: &str, power: ComicInfoTagPower) -> ComicInfoTag {
        ComicInfoTag {
            text: text.to_owned(),
            power,
        }
    }

    #[test]
    fn detailed_projection_preserves_filtering_order_and_stable_rating() {
        let info = from_gallery(request());
        assert_eq!(info.series.as_deref(), Some("Series & <title>"));
        assert_eq!(info.alternate_series, None);
        assert_eq!(
            info.writer,
            Some(vec!["group one".into(), "group, two".into()])
        );
        assert_eq!(info.penciller, None);
        assert_eq!(info.genre, Some(vec!["f:tag".into()]));
        assert_eq!(info.teams, Some(vec!["parody".into()]));
        assert_eq!(info.language_iso.as_deref(), Some("en"));
        assert_eq!(info.community_rating.as_deref(), Some("4.5"));
        assert_eq!(
            info.simple_tags,
            Some(vec![
                "group one".into(),
                "group, two".into(),
                "f:tag".into(),
                "parody".into(),
            ])
        );
    }

    #[test]
    fn simple_projection_keeps_legacy_namespace_and_original_rules() {
        let mut value = request();
        value.tag_groups = None;
        value.simple_tags = Some(vec![
            "artist:original".into(),
            "parody:original".into(),
            "male:".into(),
            "temp:ignored".into(),
            "without namespace".into(),
        ]);
        let info = from_gallery(value);
        assert_eq!(info.penciller, Some(vec!["original".into()]));
        assert_eq!(info.teams, None);
        assert_eq!(info.genre, Some(vec!["m:".into()]));
    }

    #[test]
    fn encoder_matches_legacy_xml_bytes() {
        let xml = String::from_utf8(encode_xml(&from_gallery(request())).expect("valid XML"))
            .expect("encoder emits UTF-8");
        assert_eq!(
            xml,
            concat!(
                "<?xml version='1.0' encoding='UTF-8' ?>\n",
                "<ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" ",
                "xsi:noNamespaceSchemaLocation=\"https://raw.githubusercontent.com/",
                "anansi-project/comicinfo/main/schema/v2.0/ComicInfo.xsd\">\n",
                "  <Series>Series &amp; &lt;title&gt;</Series>\n",
                "  <Writer>group one, group, two</Writer>\n",
                "  <Genre>f:tag</Genre>\n",
                "  <Web>https://e-hentai.org/g/1/0123456789/</Web>\n",
                "  <PageCount>-1</PageCount>\n",
                "  <LanguageISO>en</LanguageISO>\n",
                "  <Teams>parody</Teams>\n",
                "  <CommunityRating>4.5</CommunityRating>\n",
                "</ComicInfo>",
            )
        );
    }

    #[test]
    fn decoder_preserves_missing_empty_schema_and_unknown_child_behavior() {
        let xml = br#"<?xml version="1.0"?><ComicInfo other="accepted">
          <Writer />
          <Series>A &amp; B</Series>
          <Unknown><Series>must not replace</Series></Unknown>
          <PageCount>12</PageCount>
        </ComicInfo>"#;
        let info = decode_xml(xml).expect("compatible XML should decode");
        assert_eq!(info.series.as_deref(), Some("A & B"));
        assert_eq!(info.writer, Some(vec![String::new()]));
        assert_eq!(info.alternate_series, None);
        assert_eq!(info.page_count, 12);
        assert_eq!(info.simple_tags, Some(vec![String::new()]));

        let minimal = decode_xml(b"<ComicInfo />").expect("empty root should decode");
        assert_eq!(minimal.page_count, 0);
        assert_eq!(minimal.series, None);
        assert_eq!(minimal.simple_tags, None);
    }

    #[test]
    fn malformed_invalid_scalar_and_oversized_xml_are_fixed_errors() {
        for bytes in [
            b"".as_slice(),
            b"<Other />",
            b"<ComicInfo><PageCount>x</PageCount></ComicInfo>",
        ] {
            assert_eq!(decode_xml(bytes), Err(invalid_xml()));
        }
        assert_eq!(
            decode_xml(&vec![b'x'; MAX_COMIC_INFO_BYTES + 1]),
            Err(invalid_xml())
        );
        let mut info = from_gallery(request());
        info.series = Some("bad\0value".to_owned());
        assert_eq!(encode_xml(&info), Err(invalid_xml()));
    }

    #[test]
    fn envelopes_cover_success_protocol_and_xml_failures() {
        let built = build_gallery_envelope(request());
        assert_eq!(built.version, COMIC_INFO_API_VERSION);
        assert_eq!(built.module, COMIC_INFO_MODULE);
        assert!(built.payload.is_some());
        assert_eq!(built.error, None);

        let info = from_gallery(request());
        let encoded = encode_xml_envelope(ComicInfoEncodeRequest {
            version: COMIC_INFO_API_VERSION,
            info: info.clone(),
        });
        let xml = encoded.payload.expect("encoded XML").xml;
        assert_eq!(encoded.error, None);

        let decoded = decode_xml_envelope(ComicInfoDecodeRequest {
            version: COMIC_INFO_API_VERSION,
            xml: xml.into_vec(),
        });
        let mut expected = info;
        expected.writer = Some(vec!["group one".into(), "group".into(), "two".into()]);
        expected.simple_tags = to_simple_tags(&expected);
        assert_eq!(decoded.payload, Some(expected));
        assert_eq!(decoded.error, None);

        let unsupported_encode = encode_xml_envelope(ComicInfoEncodeRequest {
            version: 2,
            info: from_gallery(request()),
        });
        assert!(unsupported_encode.payload.is_none());
        assert!(unsupported_encode.error.is_some());
        let unsupported_decode = decode_xml_envelope(ComicInfoDecodeRequest {
            version: 2,
            xml: b"<ComicInfo />".to_vec(),
        });
        assert!(unsupported_decode.payload.is_none());
        assert!(unsupported_decode.error.is_some());

        let malformed = decode_xml_envelope(ComicInfoDecodeRequest {
            version: COMIC_INFO_API_VERSION,
            xml: b"not XML".to_vec(),
        });
        assert_eq!(malformed.error, Some(invalid_xml()));
        let mut invalid = from_gallery(request());
        invalid.series = Some("invalid\0scalar".to_owned());
        let invalid = encode_xml_envelope(ComicInfoEncodeRequest {
            version: COMIC_INFO_API_VERSION,
            info: invalid,
        });
        assert_eq!(invalid.error, Some(invalid_xml()));
    }

    #[test]
    fn projects_every_namespace_and_flattens_all_fields_in_order() {
        let mut value = request();
        value.tag_groups = Some(vec![
            group("artist", "artist"),
            group("cosplayer", "cosplayer"),
            group("group", "group"),
            group("character", "character"),
            group("parody", "parody"),
            group("location", "location"),
            group("other", "other"),
            group("female", "female"),
            group("male", "male"),
            group("mixed", "mixed"),
            group("unknown", "ignored"),
        ]);

        let info = from_gallery(value);
        assert_eq!(
            info.penciller,
            Some(vec!["artist".to_owned(), "cosplayer".to_owned()])
        );
        assert_eq!(info.writer, Some(vec!["group".to_owned()]));
        assert_eq!(info.characters, Some(vec!["character".to_owned()]));
        assert_eq!(info.teams, Some(vec!["parody".to_owned()]));
        assert_eq!(
            info.genre,
            Some(vec![
                "location".to_owned(),
                "other".to_owned(),
                "f:female".to_owned(),
                "m:male".to_owned(),
                "x:mixed".to_owned(),
            ])
        );
        assert!(
            !info
                .simple_tags
                .as_ref()
                .expect("flattened tags")
                .iter()
                .any(|tag| tag == "ignored")
        );

        let empty = ComicInfo {
            series: None,
            alternate_series: None,
            writer: None,
            penciller: None,
            genre: None,
            web: None,
            page_count: 0,
            language_iso: None,
            characters: None,
            teams: None,
            community_rating: None,
            simple_tags: None,
        };
        assert_eq!(to_simple_tags(&empty), None);
    }

    #[test]
    fn xml_round_trip_covers_all_fields_empty_tags_entities_and_misc_events() {
        let info = ComicInfo {
            series: Some("A & <B>".to_owned()),
            alternate_series: Some(String::new()),
            writer: Some(vec![String::new()]),
            penciller: Some(vec!["p1".to_owned(), "p2".to_owned()]),
            genre: Some(vec!["genre".to_owned()]),
            web: Some("https://example.invalid/?a=1&b=2".to_owned()),
            page_count: 12,
            language_iso: Some("en".to_owned()),
            characters: Some(vec!["character".to_owned()]),
            teams: Some(vec!["team".to_owned()]),
            community_rating: Some("5.0".to_owned()),
            simple_tags: None,
        };
        let xml = String::from_utf8(encode_xml(&info).expect("valid XML")).expect("UTF-8");
        assert!(xml.contains("<AlternateSeries />"));
        assert!(xml.contains("<Writer />"));
        assert!(xml.contains("A &amp; &lt;B&gt;"));
        let decoded = decode_xml(xml.as_bytes()).expect("encoded XML should decode");
        assert_eq!(decoded.series, info.series);
        assert_eq!(decoded.penciller, info.penciller);
        assert_eq!(decoded.genre, info.genre);
        assert_eq!(decoded.web, info.web);
        assert_eq!(decoded.page_count, 12);
        assert_eq!(decoded.language_iso, info.language_iso);
        assert_eq!(decoded.characters, info.characters);
        assert_eq!(decoded.teams, info.teams);
        assert_eq!(decoded.community_rating, info.community_rating);

        let event_xml = br#"<?xml version="1.0"?><?owner rust?><!--comment--><ComicInfo>
          <Series><![CDATA[A]]>&#66;&apos;&quot;&lt;&gt;&amp;</Series>
          <AlternateSeries />
          <Writer />
          <Penciller>one, two</Penciller>
          <Genre>genre</Genre>
          <Web>web</Web>
          <PageCount>3</PageCount>
          <LanguageISO>en</LanguageISO>
          <Characters>character</Characters>
          <Teams>team</Teams>
          <CommunityRating>4.0</CommunityRating>
        </ComicInfo>"#;
        let decoded = decode_xml(event_xml).expect("supported XML events should decode");
        assert_eq!(decoded.series.as_deref(), Some("AB'\"<>&"));
        assert_eq!(decoded.alternate_series.as_deref(), Some(""));
        assert_eq!(decoded.writer, Some(vec![String::new()]));
        assert_eq!(decoded.penciller, Some(vec!["one".into(), "two".into()]));
        assert_eq!(decoded.genre, Some(vec!["genre".into()]));
        assert_eq!(decoded.web.as_deref(), Some("web"));
        assert_eq!(decoded.language_iso.as_deref(), Some("en"));
        assert_eq!(decoded.characters, Some(vec!["character".into()]));
        assert_eq!(decoded.teams, Some(vec!["team".into()]));
        assert_eq!(decoded.community_rating.as_deref(), Some("4.0"));
    }

    #[test]
    fn decoder_rejects_structural_reference_and_utf8_errors() {
        let invalid: &[&[u8]] = &[
            b"text<ComicInfo />",
            b"<ComicInfo /><ComicInfo />",
            b"</ComicInfo>",
            b"<Other></Other>",
            b"<!DOCTYPE ComicInfo><ComicInfo />",
            b"<ComicInfo><Series>&unknown;</Series></ComicInfo>",
            b"<ComicInfo><Series>&#0;</Series></ComicInfo>",
            b"<ComicInfo><Series>x</Writer></ComicInfo>",
            b"<ComicInfo><Series><![CDATA[\xff]]></Series></ComicInfo>",
        ];
        for bytes in invalid {
            assert_eq!(decode_xml(bytes), Err(invalid_xml()), "{bytes:?}");
        }
    }

    fn group(namespace: &str, text: &str) -> ComicInfoTagGroup {
        ComicInfoTagGroup {
            namespace: namespace.to_owned(),
            tags: vec![tag(text, ComicInfoTagPower::Solid)],
        }
    }

    #[test]
    fn rating_special_values_and_wire_names_are_stable() {
        assert_eq!(format_rating(-0.0), "-0.0");
        assert_eq!(format_rating(f32::NAN), "NaN");
        assert_eq!(format_rating(f32::INFINITY), "Infinity");
        assert_eq!(format_rating(f32::NEG_INFINITY), "-Infinity");
        let wire = serde_json::to_value(request()).expect("request serializes");
        assert_eq!(wire["titleJpn"], json!("   "));
        assert_eq!(wire["simpleLanguage"], json!("EN"));
        assert!(wire.get("tagGroups").is_some());
    }

    #[test]
    fn unsupported_versions_return_typed_envelopes() {
        let mut value = request();
        value.version = 2;
        let envelope = build_gallery_envelope(value);
        assert_eq!(envelope.version, COMIC_INFO_API_VERSION);
        assert_eq!(envelope.module, COMIC_INFO_MODULE);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(
                "unsupported ComicInfo protocol version".to_owned()
            ))
        );
    }
}
