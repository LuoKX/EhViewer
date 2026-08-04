use std::borrow::Cow;

use quick_xml::escape::{EscapeError, resolve_xml_entity, unescape_with};

pub(crate) fn unescape(raw: &str) -> Result<Cow<'_, str>, EscapeError> {
    unescape_with(raw, resolve_entity)
}

#[inline]
const fn resolve_entity(entity: &str) -> Option<&'static str> {
    match entity.as_bytes() {
        b"nbsp" => Some(" "),
        b"times" => Some("×"),
        _ => resolve_xml_entity(entity),
    }
}
