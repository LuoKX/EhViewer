//! Pure search-query normalization and tag-keyword formatting.
//!
//! Android retains text-field state, search history, tag suggestions, and the
//! translation database. This module receives only strings resolved at that
//! boundary and returns one formatted value without platform effects.

use serde::{Deserialize, Serialize};

/// Protocol version of the search-rule request and response.
pub const SEARCH_RULE_API_VERSION: u8 = 1;

/// Stable module identifier used by the search-rule response envelope.
pub const SEARCH_RULE_MODULE: &str = "searchRule";

/// Pure search operation requested by Android.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchRuleKind {
    /// Trim the query and collapse Java-regex `\s` runs to one ASCII space.
    NormalizeQuery,
    /// Format one tag keyword for remote gallery search.
    WrapTagKeyword,
}

/// Versioned input for one search-rule operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRuleRequest {
    /// Protocol version; callers must use [`SEARCH_RULE_API_VERSION`].
    pub version: u8,
    /// Requested pure operation.
    pub kind: SearchRuleKind,
    /// Raw query or tag keyword, depending on [`Self::kind`].
    pub input: String,
    /// Android-resolved namespace translation for translated tag formatting.
    pub translated_prefix: Option<String>,
    /// Android-resolved tag translation for translated tag formatting.
    pub translated_tag: Option<String>,
}

impl SearchRuleRequest {
    /// Whether optional translation fields match the selected operation.
    #[must_use]
    pub fn has_valid_shape(&self) -> bool {
        match self.kind {
            SearchRuleKind::NormalizeQuery => {
                self.translated_prefix.is_none() && self.translated_tag.is_none()
            }
            SearchRuleKind::WrapTagKeyword => {
                self.translated_prefix.is_some() == self.translated_tag.is_some()
            }
        }
    }
}

/// Complete pure value returned for one search rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRulePlan {
    /// Operation echoed for Android response validation.
    pub kind: SearchRuleKind,
    /// Normalized query or formatted tag keyword.
    pub value: String,
}

/// Applies one validated search rule.
#[must_use]
pub fn plan_search_rule(request: &SearchRuleRequest) -> SearchRulePlan {
    debug_assert!(request.has_valid_shape());
    let value = match request.kind {
        SearchRuleKind::NormalizeQuery => normalize_query(&request.input),
        SearchRuleKind::WrapTagKeyword => wrap_tag_keyword(request),
    };
    SearchRulePlan {
        kind: request.kind,
        value,
    }
}

fn normalize_query(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_java_whitespace = false;
    for character in input.trim().chars() {
        if matches!(
            character,
            ' ' | '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r'
        ) {
            if !in_java_whitespace {
                output.push(' ');
                in_java_whitespace = true;
            }
        } else {
            output.push(character);
            in_java_whitespace = false;
        }
    }
    output
}

fn wrap_tag_keyword(request: &SearchRuleRequest) -> String {
    if let (Some(prefix), Some(tag)) = (&request.translated_prefix, &request.translated_tag) {
        return format!("{prefix}：{tag}");
    }
    let (prefix, tag) = request
        .input
        .split_once(':')
        .unwrap_or(("", &request.input));
    if request.input.contains(' ') {
        format!("{prefix}:\"{tag}$\"")
    } else {
        format!("{}$", request.input)
    }
}

#[cfg(test)]
mod tests {
    use super::{SEARCH_RULE_API_VERSION, SearchRuleKind, SearchRuleRequest, plan_search_rule};

    #[test]
    fn normalizes_the_legacy_java_whitespace_set() {
        let normalized = request(
            SearchRuleKind::NormalizeQuery,
            "\u{2003} foo\t\nbar  \u{2003}",
        );
        assert_eq!(plan_search_rule(&normalized).value, "foo bar");

        let unicode_whitespace = request(SearchRuleKind::NormalizeQuery, "foo\u{2003}bar");
        assert_eq!(
            plan_search_rule(&unicode_whitespace).value,
            "foo\u{2003}bar"
        );
    }

    #[test]
    fn formats_plain_spaced_and_translated_tags() {
        assert_eq!(
            plan_search_rule(&request(SearchRuleKind::WrapTagKeyword, "artist:foo")).value,
            "artist:foo$"
        );
        assert_eq!(
            plan_search_rule(&request(SearchRuleKind::WrapTagKeyword, "artist:foo bar")).value,
            "artist:\"foo bar$\""
        );
        let translated = SearchRuleRequest {
            version: SEARCH_RULE_API_VERSION,
            kind: SearchRuleKind::WrapTagKeyword,
            input: "artist:foo".to_owned(),
            translated_prefix: Some("艺术家".to_owned()),
            translated_tag: Some("示例".to_owned()),
        };
        assert_eq!(plan_search_rule(&translated).value, "艺术家：示例");
    }

    #[test]
    fn validates_optional_translation_shape() {
        let mut request = request(SearchRuleKind::NormalizeQuery, "query");
        assert!(request.has_valid_shape());
        request.translated_prefix = Some("prefix".to_owned());
        assert!(!request.has_valid_shape());

        request.kind = SearchRuleKind::WrapTagKeyword;
        assert!(!request.has_valid_shape());
        request.translated_tag = Some("tag".to_owned());
        assert!(request.has_valid_shape());
    }

    fn request(kind: SearchRuleKind, input: &str) -> SearchRuleRequest {
        SearchRuleRequest {
            version: SEARCH_RULE_API_VERSION,
            kind,
            input: input.to_owned(),
            translated_prefix: None,
            translated_tag: None,
        }
    }
}
