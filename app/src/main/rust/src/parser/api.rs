use crate::EhError;
use crate::parser::detail::{GalleryTagGroup, parse_tag_groups};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use tl::ParserOptions;

pub fn parse_vote_tag(str: &str) -> Result<Vec<GalleryTagGroup>> {
    let result = serde_json::from_str::<Value>(str)?;
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        bail!(EhError::Error(error.to_string()))
    };
    let tagpane = result
        .get("tagpane")
        .and_then(Value::as_str)
        .context("Failed to get tagpane")?;
    let dom = tl::parse(tagpane, ParserOptions::default())?;
    parse_tag_groups(&dom, dom.parser(), true)
}

#[cfg(test)]
mod tests {
    use crate::EhError;
    use crate::parser::detail::{PowerStatus, VoteStatus};

    use super::parse_vote_tag;

    #[test]
    fn legacy_parser_preserves_empty_text_aliases_and_unknown_vote_classes() {
        let input = r#"{"error":null,"tagpane":"<table><tr><td>artist:</td><td><div class='gtl'><span class='unknown'></span> | alias</div></td></tr></table>"}"#;
        let mut groups = parse_vote_tag(input).expect("legacy tag pane should parse");
        let mut group = groups.remove(0);
        let tag = group.tags.remove(0);

        assert_eq!(group.namespace, "artist");
        assert_eq!(tag.text, "");
        assert!(matches!(tag.power, PowerStatus::Active));
        assert!(matches!(tag.vote, VoteStatus::None));
    }

    #[test]
    fn legacy_parser_keeps_error_precedence_and_empty_results() {
        let error = parse_vote_tag(r#"{"error":"","tagpane":"<table></table>"}"#)
            .expect_err("string error should win");
        assert!(matches!(
            error.downcast_ref::<EhError>(),
            Some(EhError::Error(message)) if message.is_empty()
        ));

        for input in [
            r#"{"tagpane":"No tags have been added yet."}"#,
            r#"{"error":1,"tagpane":"<table></table>"}"#,
        ] {
            assert!(
                parse_vote_tag(input)
                    .expect("legacy-compatible empty pane should parse")
                    .is_empty()
            );
        }
    }

    #[test]
    fn legacy_parser_rejects_missing_panes_and_missing_vote_children() {
        assert!(parse_vote_tag("{}").is_err());
        assert!(
            parse_vote_tag(
                r#"{"tagpane":"<table><tr><td>artist:</td><td><div class='gt'>tag</div></td></tr></table>"}"#
            )
            .is_err()
        );
    }
}
