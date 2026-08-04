//! Gallery List metadata API response parsing.
//!
//! Android retains request construction, credentials, HTTP/session state, batching,
//! response-to-object matching, date presentation, language derivation, and mutation.
//! Rust receives only response JSON and emits normalized metadata in source order.

use eh_domain::contracts::{ApiEnvelope, GalleryMetadataBatch, GalleryMetadataItem, RustError};
use serde::Deserialize;
use serde_json::Value;

use super::{legacy_f64, legacy_i64, list::category_value};

const INVALID_RESPONSE: &str = "Gallery metadata response is invalid";
const EHGT_THUMBNAIL_PREFIX: &str = "https://ehgt.org/";
const EX_V1_THUMBNAIL_PREFIX: &str = "https://s.exhentai.org/t/";
const EX_THUMBNAIL_PREFIX: &str = "https://s.exhentai.org/";
const KNOWN_ENTITIES: [(&str, &str); 7] = [
    ("&amp;", "&"),
    ("&lt;", "<"),
    ("&gt;", ">"),
    ("&quot;", "\""),
    ("&#039;", "'"),
    ("&nbsp;", "\u{a0}"),
    ("&times;", "×"),
];

/// Current Gallery List metadata bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryMetadata";

#[derive(Deserialize)]
struct MetadataResponse {
    #[serde(rename = "gmetadata")]
    items: Vec<RawMetadataItem>,
}

#[derive(Deserialize)]
struct RawMetadataItem {
    gid: i64,
    title: String,
    title_jpn: String,
    category: String,
    thumb: String,
    uploader: Option<String>,
    posted: Value,
    filecount: Value,
    rating: Value,
    tags: Vec<String>,
}

/// Parses one `gdata` response into a versioned, body-free-error envelope.
#[must_use]
pub fn parse_envelope(input: &str) -> ApiEnvelope<GalleryMetadataBatch> {
    match parse(input) {
        Ok(batch) => ApiEnvelope::ok(API_VERSION, MODULE, batch),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses and normalizes a complete Gallery List metadata response.
///
/// Compatibility follows the frozen Kotlin serializer: unknown fields are ignored,
/// duplicate JSON fields use their final value, API numeric strings are accepted, nullable
/// or missing uploaders become `None`, item order and duplicate gallery identifiers are
/// preserved, and all required fields must decode before Android mutates any list entry.
/// Only the seven entities recognized by the legacy `unescapeXml` helper are decoded.
///
/// # Errors
///
/// Returns one fixed [`RustError::Error`] for malformed JSON, an invalid root or item
/// shape, integer overflow, or a rating not representable as a finite `f32`. Response
/// text and gallery capabilities are never copied into the error.
pub fn parse(input: &str) -> Result<GalleryMetadataBatch, RustError> {
    let value = serde_json::from_str::<Value>(input).map_err(|_| invalid_response())?;
    let response =
        serde_json::from_value::<MetadataResponse>(value).map_err(|_| invalid_response())?;
    let mut items = Vec::with_capacity(response.items.len());
    for raw in response.items {
        let Some(posted) = legacy_i64(&raw.posted) else {
            return Err(invalid_response());
        };
        let Some(page_count) = legacy_i64(&raw.filecount).and_then(|count| i32::try_from(count).ok())
        else {
            return Err(invalid_response());
        };
        let Some(rating) = legacy_f64(&raw.rating) else {
            return Err(invalid_response());
        };
        #[allow(clippy::cast_possible_truncation)]
        let rating = rating as f32;
        if !rating.is_finite() {
            return Err(invalid_response());
        }
        items.push(GalleryMetadataItem {
            gallery_id: raw.gid,
            title: unescape_known(&raw.title),
            title_japanese: unescape_known(&raw.title_jpn),
            category: category_value(&raw.category.to_ascii_lowercase()),
            thumbnail_key: metadata_thumbnail_key(&raw.thumb),
            uploader: raw.uploader.map(|value| unescape_known(&value)),
            posted,
            rating,
            tags: raw.tags,
            page_count,
        });
    }
    Ok(GalleryMetadataBatch { items })
}

fn invalid_response() -> RustError {
    RustError::Error(INVALID_RESPONSE.to_owned())
}

fn metadata_thumbnail_key(url: &str) -> String {
    let key = url.strip_prefix(EHGT_THUMBNAIL_PREFIX).unwrap_or(url);
    let key = key.strip_prefix(EX_V1_THUMBNAIL_PREFIX).unwrap_or(key);
    key.strip_prefix(EX_THUMBNAIL_PREFIX)
        .unwrap_or(key)
        .to_owned()
}

fn unescape_known(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remainder = input;
    while let Some(offset) = remainder.find('&') {
        output.push_str(&remainder[..offset]);
        remainder = &remainder[offset..];
        if let Some((entity, replacement)) = KNOWN_ENTITIES
            .iter()
            .find(|(entity, _)| remainder.starts_with(entity))
        {
            output.push_str(replacement);
            remainder = &remainder[entity.len()..];
        } else {
            output.push('&');
            remainder = &remainder[1..];
        }
    }
    output.push_str(remainder);
    output
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryMetadataBatch, GalleryMetadataItem, RustError};

    use super::{API_VERSION, INVALID_RESPONSE, MODULE, parse, parse_envelope, unescape_known};
    use crate::test_support::to_hex;

    const RESPONSE: &str = include_str!("../../test_data/gallery_metadata.json");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656f67616c6c6572794d657461646174",
        "61677061796c6f6164a1656974656d7382aa636769641865657469746c65781a52",
        "6f636b202620526f6c6c2026756e6b6e6f776e3b20266c743b687469746c654a",
        "706e75e697a5e69cac203ce789b9e99b863e2022e4b880226863617465676f72",
        "7908687468756d624b65796861622f632e6a70676875706c6f6164657277416c",
        "69636520274127c2a0c39720266d7973746572793b66706f737465641a6553f100",
        "66726174696e67f944c06a73696d706c655461677382706c616e67756167653a",
        "656e676c6973686d6172746973743a73616d706c656570616765730caa63676964",
        "26657469746c6560687469746c654a706e606863617465676f7279190800687468",
        "756d624b6579676b65792e6a70676875706c6f61646572f666706f7374656420",
        "66726174696e67f980006a73696d706c6554616773806570616765733a7fffffff",
        "656572726f72f6",
    );

    #[test]
    fn parses_and_normalizes_the_offline_fixture() {
        let batch = parse(RESPONSE).expect("checked-in gallery metadata should parse");
        assert_eq!(batch.items.len(), 2);
        assert_eq!(
            batch.items[0],
            GalleryMetadataItem {
                gallery_id: 101,
                title: "Rock & Roll &unknown; &lt;".to_owned(),
                title_japanese: "日本 <特集> \"一\"".to_owned(),
                category: 0x8,
                thumbnail_key: "ab/c.jpg".to_owned(),
                uploader: Some("Alice 'A'\u{a0}× &mystery;".to_owned()),
                posted: 1_700_000_000,
                rating: 4.75,
                tags: vec!["language:english".to_owned(), "artist:sample".to_owned()],
                page_count: 12,
            }
        );
        let second = &batch.items[1];
        assert_eq!(second.gallery_id, -7);
        assert_eq!(second.category, 0x800);
        assert_eq!(second.thumbnail_key, "key.jpg");
        assert_eq!(second.uploader, None);
        assert_eq!(second.rating.to_bits(), (-0.0_f32).to_bits());
        assert_eq!(second.page_count, i32::MIN);
    }

    #[test]
    fn accepts_empty_batches_unknown_fields_and_preserves_item_order_and_duplicates() {
        assert_eq!(
            parse(r#"{"gmetadata":[],"ignored":true}"#)
                .expect("empty metadata should remain a success"),
            GalleryMetadataBatch { items: Vec::new() }
        );
        let input = format!(
            r#"{{"gmetadata":[{},{}]}}"#,
            raw_item(9, "first", "misc", "first.jpg", "null", "0", 1),
            raw_item(9, "second", "MISC", "second.jpg", "null", "5", 2),
        );
        let batch = parse(&input).expect("duplicates should remain source ordered");
        assert_eq!(
            batch
                .items
                .iter()
                .map(|item| item.gallery_id)
                .collect::<Vec<_>>(),
            [9, 9]
        );
        assert_eq!(
            batch
                .items
                .iter()
                .map(|item| item.title.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(batch.items[1].rating.to_bits(), 5.0_f32.to_bits());
    }

    #[test]
    fn duplicate_json_fields_use_the_last_value() {
        let input = r#"{"gmetadata":[{"gid":1,"gid":2,"title":"old","title":"new","title_jpn":"","category":"misc","thumb":"old","thumb":"https://ehgt.org/new","uploader":null,"posted":1,"posted":2,"filecount":3,"filecount":4,"rating":1,"rating":2,"tags":[],"tags":["last"]}]}"#;
        let item = parse(input)
            .expect("last duplicate fields should win")
            .items
            .remove(0);
        assert_eq!(item.gallery_id, 2);
        assert_eq!(item.title, "new");
        assert_eq!(item.thumbnail_key, "new");
        assert_eq!(item.posted, 2);
        assert_eq!(item.page_count, 4);
        assert_eq!(item.rating.to_bits(), 2.0_f32.to_bits());
        assert_eq!(item.tags, ["last"]);
    }

    #[test]
    fn preserves_the_exact_known_entity_category_and_thumbnail_rules() {
        assert_eq!(
            unescape_known("&amp;&lt;&gt;&quot;&#039;&nbsp;&times;&unknown;&amp;lt; plain"),
            "&<>\"'\u{a0}×&unknown;&lt; plain"
        );
        let cases = [
            ("misc", 0x1),
            ("Doujinshi", 0x2),
            ("MANGA", 0x4),
            ("ArtistCG", 0x8),
            ("Artist CG Sets", 0x8),
            ("Game CG", 0x10),
            ("Image Sets", 0x20),
            ("Cosplay", 0x40),
            ("Asian Porn", 0x80),
            ("Non-H", 0x100),
            ("Western", 0x200),
            ("Private", 0x400),
            (" misc ", 0x800),
            ("unknown", 0x800),
        ];
        for (index, (category, expected)) in cases.into_iter().enumerate() {
            let input = format!(
                r#"{{"gmetadata":[{}]}}"#,
                raw_item(
                    i64::try_from(index).expect("small case index should fit i64"),
                    "",
                    category,
                    "relative.jpg",
                    "null",
                    "0",
                    0,
                ),
            );
            assert_eq!(
                parse(&input).expect("category alias should parse").items[0].category,
                expected
            );
        }
        for (source, expected) in [
            ("https://ehgt.org/a.jpg", "a.jpg"),
            ("https://ehgt.org/t/a.jpg", "t/a.jpg"),
            ("https://s.exhentai.org/t/b.jpg", "b.jpg"),
            ("https://s.exhentai.org/c.jpg", "c.jpg"),
            ("t/relative.jpg", "t/relative.jpg"),
            ("relative.jpg", "relative.jpg"),
        ] {
            let input = format!(
                r#"{{"gmetadata":[{}]}}"#,
                raw_item(1, "", "misc", source, "null", "0", 0),
            );
            assert_eq!(
                parse(&input).expect("thumbnail variant should parse").items[0].thumbnail_key,
                expected
            );
        }
    }

    #[test]
    fn rejects_every_invalid_shape_with_one_body_free_error() {
        let secret = "sensitive-gallery-token";
        let valid = raw_item(1, "title", "misc", "thumb", "null", "4.5", 1);
        let wrap = |item: String| format!(r#"{{"gmetadata":[{item}]}}"#);
        let inputs = [
            String::new(),
            "null".to_owned(),
            "[]".to_owned(),
            "{}".to_owned(),
            r#"{"gmetadata":null}"#.to_owned(),
            r#"{"gmetadata":{}}"#.to_owned(),
            r#"{"gmetadata":[null]}"#.to_owned(),
            r#"{"gmetadata":[{}]}"#.to_owned(),
            wrap(valid.replace(r#""gid":1"#, r#""gid":9223372036854775808"#)),
            wrap(valid.replace(r#""filecount":1"#, r#""filecount":2147483648"#)),
            wrap(valid.replace(r#""rating":4.5"#, r#""rating":"4.5""#)),
            wrap(valid.replace(r#""rating":4.5"#, r#""rating":1e39"#)),
            wrap(valid.replace(r#""tags":[]"#, r#""tags":[1]"#)),
            wrap(valid.replace(r#""uploader":null"#, r#""uploader":1"#)),
            format!(r#"{{"error":"{secret}"}}"#),
        ];
        for input in inputs {
            let error = parse(&input).expect_err("invalid gallery metadata should fail");
            assert_eq!(error, RustError::Error(INVALID_RESPONSE.to_owned()));
            assert!(!error.to_string().contains(secret));
        }
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = parse_envelope(RESPONSE);
        assert_eq!(envelope.version, API_VERSION);
        assert_eq!(envelope.module, MODULE);
        assert_eq!(
            envelope.payload.as_ref().map(|batch| batch.items.len()),
            Some(2)
        );
        assert_eq!(
            to_hex(&serde_cbor::to_vec(&envelope).expect("V1 response should encode")),
            V1_GOLDEN_CBOR_HEX
        );

        let failure = parse_envelope("not JSON");
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(INVALID_RESPONSE.to_owned()))
        );
    }

    fn raw_item(
        gid: i64,
        title: &str,
        category: &str,
        thumb: &str,
        uploader: &str,
        rating: &str,
        filecount: i32,
    ) -> String {
        format!(
            r#"{{"gid":{gid},"title":"{title}","title_jpn":"","category":"{category}","thumb":"{thumb}","uploader":{uploader},"posted":0,"filecount":{filecount},"rating":{rating},"tags":[]}}"#
        )
    }
}
