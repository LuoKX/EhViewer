use std::borrow::Cow;

use quick_xml::escape::{EscapeError, resolve_xml_entity, unescape_with};

const LEGACY_ENTITIES: [(&str, &str); 7] = [
    ("&amp;", "&"),
    ("&lt;", "<"),
    ("&gt;", ">"),
    ("&quot;", "\""),
    ("&#039;", "'"),
    ("&nbsp;", " "),
    ("&times;", "×"),
];

pub(crate) fn unescape(raw: &str) -> Result<Cow<'_, str>, EscapeError> {
    unescape_with(raw, resolve_entity)
}

/// Reproduces the frozen Kotlin `unescapeXml` replacement set in one pass.
///
/// Unknown entities stay unchanged, and replacement output is never scanned again.
pub(crate) fn unescape_legacy_entities(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remainder = input;
    while let Some(offset) = remainder.find('&') {
        output.push_str(&remainder[..offset]);
        remainder = &remainder[offset..];
        if let Some((entity, replacement)) = LEGACY_ENTITIES
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

#[inline]
const fn resolve_entity(entity: &str) -> Option<&'static str> {
    match entity.as_bytes() {
        b"nbsp" => Some(" "),
        b"times" => Some("×"),
        _ => resolve_xml_entity(entity),
    }
}
