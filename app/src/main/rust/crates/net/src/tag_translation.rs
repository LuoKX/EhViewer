//! Ordered, Rust-owned tag translation dictionaries and Kotlin-compatible suggestion scoring.
//!
//! Android retains HTTP, files, locale settings and index publication. Queries return only the
//! displayed top-N rows; the complete dictionary never crosses the CBOR boundary.

use std::collections::HashMap;
use std::fmt;
use std::io::Read;

use eh_domain::contracts::RustError;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

/// Protocol version of tag index operations.
pub const API_VERSION: u8 = 1;
/// Stable suggestion envelope discriminator.
pub const MODULE: &str = "tagTranslation";
/// Maximum JSON input size accepted by the descriptor adapter.
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum number of suggestions returned by one UI query.
pub const MAX_SUGGESTIONS: usize = 50;

/// Versioned input for one query against an immutable native index.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestionRequest {
    /// Contract version.
    pub version: u8,
    /// Exact input including optional search modifiers.
    pub keyword: String,
    /// Whether matching translated hints is enabled by Android's locale/settings.
    pub translate: bool,
    /// Number of visible rows requested, from zero through fifty.
    pub limit: usize,
}

/// One scored row in stable source order when scores tie.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TagSuggestion {
    /// Search-ready tag including modifiers and namespace prefix.
    pub tag: String,
    /// Translation shown when translated matching is enabled.
    pub hint: Option<String>,
    /// Legacy single-precision, UTF-16-length-based score.
    pub score: f32,
}

/// Immutable ordered dictionary, safe to share across concurrent translation and query calls.
#[derive(Debug)]
pub struct TagIndex {
    groups: Vec<Group>,
    positions: HashMap<String, usize>,
}

#[derive(Debug)]
struct Group {
    prefix: String,
    tags: Vec<(String, String)>,
    positions: HashMap<String, usize>,
}

// serde_json's default object map sorts keys. Preserve insertion order and last-value-wins
// duplicates instead, matching Kotlin JsonObject/LinkedHashMap and stable suggestion tie ordering.
struct OrderedMap<T>(Vec<(String, T)>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for OrderedMap<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for OrderedVisitor<T> {
            type Value = OrderedMap<T>;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an ordered string-keyed object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                let mut positions = HashMap::<String, usize>::new();
                while let Some((key, value)) = access.next_entry::<String, T>()? {
                    if let Some(&index) = positions.get(&key) {
                        values[index] = (key, value);
                    } else {
                        positions.insert(key.clone(), values.len());
                        values.push((key, value));
                    }
                }
                Ok(OrderedMap(values))
            }
        }
        deserializer.deserialize_map(OrderedVisitor(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
struct RemoteDictionary {
    data: Vec<RemoteGroup>,
}
#[derive(Deserialize)]
struct RemoteGroup {
    namespace: String,
    data: OrderedMap<RemoteTag>,
}
#[derive(Deserialize)]
struct RemoteTag {
    name: String,
}

impl TagIndex {
    /// Parses the remote dataset or an existing Kotlin-produced cached prefix/name map.
    ///
    /// # Errors
    /// Returns one body-free error for malformed JSON or an unsupported remote namespace.
    pub fn parse(reader: impl Read, remote: bool) -> Result<Self, RustError> {
        let groups = if remote {
            let source: RemoteDictionary =
                serde_json::from_reader(reader).map_err(|_| invalid())?;
            source
                .data
                .into_iter()
                .map(|group| {
                    let prefix = namespace(&group.namespace).ok_or_else(invalid)?;
                    let tags = group
                        .data
                        .0
                        .into_iter()
                        .map(|(tag, value)| (tag, value.name))
                        .collect();
                    Ok((prefix.to_owned(), tags))
                })
                .collect::<Result<Vec<_>, RustError>>()?
        } else {
            let source: OrderedMap<OrderedMap<String>> =
                serde_json::from_reader(reader).map_err(|_| invalid())?;
            source
                .0
                .into_iter()
                .map(|(prefix, tags)| (prefix, tags.0))
                .collect()
        };
        let mut index = Self {
            groups: Vec::new(),
            positions: HashMap::new(),
        };
        for (prefix, tags) in groups {
            if prefix.len() > 128
                || tags
                    .iter()
                    .any(|(tag, hint)| tag.len() > 4096 || hint.len() > 64 * 1024)
            {
                return Err(invalid());
            }
            let positions = tags
                .iter()
                .enumerate()
                .map(|(index, (tag, _))| (tag.clone(), index))
                .collect();
            let group = Group {
                prefix: prefix.clone(),
                tags,
                positions,
            };
            if let Some(&position) = index.positions.get(&prefix) {
                index.groups[position] = group;
            } else {
                index.positions.insert(prefix, index.groups.len());
                index.groups.push(group);
            }
        }
        Ok(index)
    }

    /// Looks up and Kotlin-trims one translation without copying dictionary storage.
    #[must_use]
    pub fn translation(&self, prefix: &str, tag: &str) -> Option<&str> {
        let group = &self.groups[*self.positions.get(prefix)?];
        let value = group.tags[*group.positions.get(tag)?]
            .1
            .trim_matches(kotlin_whitespace);
        (!value.is_empty()).then_some(value)
    }

    /// Returns the same visible prefix as the legacy complete list, with bounded JNI output.
    ///
    /// # Errors
    /// Rejects unsupported versions, oversized keywords and limits above fifty.
    pub fn suggest(&self, request: &SuggestionRequest) -> Result<Vec<TagSuggestion>, RustError> {
        if request.version != API_VERSION
            || request.keyword.len() > 16 * 1024
            || request.limit > MAX_SUGGESTIONS
        {
            return Err(invalid());
        }
        let mut keyword = request.keyword.as_str();
        for prefix in ["-", "~", "tag:", "weak:"] {
            keyword = keyword.strip_prefix(prefix).unwrap_or(keyword);
        }
        if keyword.is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        let modifier = &request.keyword[..request.keyword.len() - keyword.len()];
        let (name, tag) = keyword.split_once(':').unwrap_or((keyword, ""));
        let prefix = namespace(name).unwrap_or(name);
        let specific = if !tag.is_empty() && prefix != "n" {
            self.positions.get(prefix).copied()
        } else {
            None
        };
        let mut rows = Vec::new();
        if let Some(position) = specific {
            append_matches(
                &mut rows,
                &self.groups[position],
                tag,
                modifier,
                request.translate,
                false,
                request.limit,
            );
        } else {
            for group in &self.groups {
                append_matches(
                    &mut rows,
                    group,
                    keyword,
                    modifier,
                    request.translate,
                    true,
                    request.limit,
                );
            }
        }
        rows.truncate(request.limit);
        Ok(rows)
    }
}

fn append_matches(
    rows: &mut Vec<TagSuggestion>,
    group: &Group,
    keyword: &str,
    modifier: &str,
    translate: bool,
    weighted: bool,
    limit: usize,
) {
    let word_query = format!(" {keyword}");
    for (tag, hint) in &group.tags {
        let score = if starts_ignore_case(tag, keyword) {
            utf16_length(tag)
        } else if contains_ignore_case(tag, &word_query) {
            utf16_length(tag) * 2.0
        } else if translate {
            if starts_ignore_case(hint, keyword) {
                utf16_length(hint)
            } else if contains_ignore_case(hint, keyword) {
                utf16_length(hint) * 2.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        if score == 0.0 {
            continue;
        }
        let namespace_row = weighted && group.prefix == "n";
        let score = if namespace_row {
            0.0
        } else if weighted {
            score / namespace_score(&group.prefix)
        } else {
            score
        };
        // Upper-bound insertion preserves source order for equal scores. Only top-N rows allocate.
        let position = rows.partition_point(|row| row.score <= score);
        if position >= limit {
            continue;
        }
        let text = if namespace_row {
            format!("{modifier}{tag}:")
        } else {
            format!("{modifier}{}:{tag}", group.prefix)
        };
        rows.insert(
            position,
            TagSuggestion {
                tag: text,
                hint: translate.then(|| hint.clone()),
                score,
            },
        );
        rows.truncate(limit);
    }
}

fn namespace(value: &str) -> Option<&'static str> {
    match value {
        "rows" => Some("n"),
        "artist" => Some("a"),
        "cosplayer" => Some("cos"),
        "character" => Some("c"),
        "female" => Some("f"),
        "group" => Some("g"),
        "language" => Some("l"),
        "location" => Some("loc"),
        "male" => Some("m"),
        "mixed" => Some("x"),
        "other" => Some("o"),
        "parody" => Some("p"),
        "reclass" => Some("r"),
        _ => None,
    }
}
fn namespace_score(value: &str) -> f32 {
    match value {
        "loc" | "o" => 10.0,
        "f" => 9.0,
        "m" => 8.5,
        "x" => 8.0,
        "p" => 3.3,
        "c" => 2.8,
        "a" => 2.5,
        "cos" => 2.4,
        "g" => 2.2,
        "l" => 2.0,
        _ => 1.0,
    }
}
#[allow(
    clippy::cast_precision_loss,
    reason = "legacy Kotlin scores convert UTF-16 Int lengths to Float"
)]
fn utf16_length(value: &str) -> f32 {
    value.encode_utf16().count() as f32
}
fn simple_fold(value: char) -> char {
    let mut upper = value.to_uppercase();
    let first = upper.next().unwrap();
    let upper = if upper.next().is_none() { first } else { value };
    upper.to_lowercase().next().unwrap()
}
fn starts_ignore_case(value: &str, query: &str) -> bool {
    let mut values = value.chars();
    query.chars().all(|query| {
        values
            .next()
            .is_some_and(|value| simple_fold(value) == simple_fold(query))
    })
}
fn contains_ignore_case(value: &str, query: &str) -> bool {
    value
        .char_indices()
        .any(|(offset, _)| starts_ignore_case(&value[offset..], query))
}
fn kotlin_whitespace(value: char) -> bool {
    (value.is_whitespace() && value != '\u{0085}') || matches!(value, '\u{001c}'..='\u{001f}')
}
fn invalid() -> RustError {
    RustError::Error("Invalid tag translation input".to_owned())
}

#[cfg(test)]
mod tests;
