//! Shared gallery-tag table parsing.
//!
//! Gallery Detail and the tag-vote API expose the same row grammar with two
//! historical differences. Detail requires non-empty tag text but permits a
//! missing vote marker. Tag-vote responses preserve empty text but require the
//! first element child that the legacy parser consumed. Keeping that distinction
//! explicit prevents reuse from silently tightening or widening either protocol.

use eh_domain::contracts::{
    GalleryTag, GalleryTagGroup, GalleryTagPower, GalleryTagVote, RustError,
};
use tl::{HTMLTag, Node, NodeHandle, Parser, VDom};

use crate::html::first_element_child;
use crate::text::unescape;

/// Source grammar applied to otherwise shared tag rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TagGrammar {
    /// Gallery Detail requires visible tag text and tolerates no vote child.
    Detail,
    /// Tag-vote responses permit empty text but require an element vote child.
    VoteResponse,
}

/// Parses the `#taglist` table embedded in a Gallery Detail response.
pub(super) fn parse_detail(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<Vec<GalleryTagGroup>, RustError> {
    let table = dom
        .get_element_by_id("taglist")
        .and_then(|handle| handle.get(parser))
        .and_then(Node::as_tag)
        .ok_or_else(|| RustError::Error("gallery tag table is missing".to_owned()))?;
    let Some(rows) = table.query_selector(parser, "tr") else {
        return Err(RustError::Error("gallery tag row query failed".to_owned()));
    };
    rows.map(|row| parse_group(row, parser, TagGrammar::Detail))
        .collect()
}

/// Parses tag rows whose containing table is the response fragment root.
pub(super) fn parse_vote_response(
    dom: &VDom<'_>,
    parser: &Parser<'_>,
) -> Result<Vec<GalleryTagGroup>, RustError> {
    let Some(rows) = dom.query_selector("tr") else {
        return Err(RustError::Error("gallery tag row query failed".to_owned()));
    };
    rows.filter(|row| row.get(parser).and_then(Node::children).is_some())
        .map(|row| parse_group(row, parser, TagGrammar::VoteResponse))
        .collect()
}

fn parse_group(
    row: NodeHandle,
    parser: &Parser<'_>,
    grammar: TagGrammar,
) -> Result<GalleryTagGroup, RustError> {
    let Some(children) = row.get(parser).and_then(Node::children) else {
        return Err(RustError::Error("gallery tag row is malformed".to_owned()));
    };
    let mut fields = children
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser).and_then(Node::as_tag));
    let namespace = fields
        .next()
        .and_then(|field| {
            field
                .inner_text(parser)
                .strip_suffix(':')
                .map(str::to_owned)
        })
        .ok_or_else(|| RustError::Error("gallery tag namespace is malformed".to_owned()))?;
    let tags = fields
        .next()
        .ok_or_else(|| RustError::Error("gallery tag values are missing".to_owned()))?
        .children()
        .top()
        .iter()
        .filter_map(|handle| handle.get(parser).and_then(Node::as_tag))
        .map(|tag| parse_tag(tag, parser, grammar))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GalleryTagGroup { namespace, tags })
}

fn parse_tag(
    tag: &HTMLTag<'_>,
    parser: &Parser<'_>,
    grammar: TagGrammar,
) -> Result<GalleryTag, RustError> {
    let raw = tag.inner_text(parser);
    let decoded = unescape(&raw)
        .map_err(|_| RustError::Error("gallery tag contains an invalid entity".to_owned()))?;
    let text = decoded.split('|').next().map(str::trim).unwrap_or_default();
    if grammar == TagGrammar::Detail && text.is_empty() {
        return Err(RustError::Error("gallery tag text is empty".to_owned()));
    }
    let power = match tag
        .attributes()
        .class()
        .and_then(|class| class.try_as_utf8_str())
    {
        Some("gt") => GalleryTagPower::Solid,
        Some("gtl") => GalleryTagPower::Active,
        Some("gtw") => GalleryTagPower::Weak,
        _ => {
            return Err(RustError::Error(
                "gallery tag power is malformed".to_owned(),
            ));
        }
    };
    let vote_child = first_element_child(tag, parser);
    if grammar == TagGrammar::VoteResponse && vote_child.is_none() {
        return Err(RustError::Error(
            "gallery tag vote marker is missing".to_owned(),
        ));
    }
    let vote = match vote_child
        .and_then(|vote| vote.attributes().class())
        .and_then(|class| class.try_as_utf8_str())
    {
        Some("tup") => GalleryTagVote::Up,
        Some("tdn") => GalleryTagVote::Down,
        _ => GalleryTagVote::None,
    };
    Ok(GalleryTag {
        text: text.to_owned(),
        power,
        vote,
    })
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{GalleryTagPower, GalleryTagVote, RustError};
    use tl::ParserOptions;

    use super::{TagGrammar, parse_detail, parse_group};

    #[test]
    fn detail_entrypoint_requires_taglist_and_parses_its_rows() {
        let html = "<table id='taglist'><tr><td>artist:</td><td><div class='gt'>artist</div></td></tr></table>";
        let dom = tl::parse(html, ParserOptions::default().track_ids())
            .expect("detail tag HTML should parse");
        let groups = parse_detail(&dom, dom.parser()).expect("detail tag table should parse");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].namespace, "artist");
        assert_eq!(groups[0].tags[0].text, "artist");

        let missing = tl::parse("<table></table>", ParserOptions::default().track_ids())
            .expect("missing-table HTML should parse");
        assert_eq!(
            parse_detail(&missing, missing.parser()),
            Err(RustError::Error("gallery tag table is missing".to_owned()))
        );
    }

    #[test]
    fn shared_grammar_preserves_alias_stripping_entities_power_vote_and_source_order() {
        let html = concat!(
            "<table><tr><td>artist:</td><td>",
            "<div class='gtl'><span class='tup'></span>Alice &amp; Bob | alias</div>",
            "<div class='gtw'><span class='tdn'></span>Second</div>",
            "<div class='gt'><span></span>Third</div>",
            "</td></tr></table>",
        );
        let dom = tl::parse(html, ParserOptions::default()).expect("tag HTML should parse");
        let parser = dom.parser();
        let row = dom
            .query_selector("tr")
            .and_then(|mut rows| rows.next())
            .expect("tag row should exist");
        let group = parse_group(row, parser, TagGrammar::VoteResponse)
            .expect("shared tag grammar should parse");

        assert_eq!(group.namespace, "artist");
        assert_eq!(group.tags.len(), 3);
        assert_eq!(group.tags[0].text, "Alice & Bob");
        assert_eq!(group.tags[0].power, GalleryTagPower::Active);
        assert_eq!(group.tags[0].vote, GalleryTagVote::Up);
        assert_eq!(group.tags[1].power, GalleryTagPower::Weak);
        assert_eq!(group.tags[1].vote, GalleryTagVote::Down);
        assert_eq!(group.tags[2].power, GalleryTagPower::Solid);
        assert_eq!(group.tags[2].vote, GalleryTagVote::None);
    }

    #[test]
    fn detail_and_vote_response_keep_their_distinct_text_and_marker_rules() {
        let empty_text = "<table><tr><td>artist:</td><td><div class='gt'><span></span> | alias</div></td></tr></table>";
        assert!(parse_one(empty_text, TagGrammar::VoteResponse).is_ok());
        assert_eq!(
            parse_one(empty_text, TagGrammar::Detail),
            Err(RustError::Error("gallery tag text is empty".to_owned()))
        );

        let missing_marker =
            "<table><tr><td>artist:</td><td><div class='gt'>artist</div></td></tr></table>";
        assert!(parse_one(missing_marker, TagGrammar::Detail).is_ok());
        assert_eq!(
            parse_one(missing_marker, TagGrammar::VoteResponse),
            Err(RustError::Error(
                "gallery tag vote marker is missing".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_every_malformed_shared_row_field() {
        let cases = [
            ("<table><tr></tr></table>", "namespace"),
            (
                "<table><tr><td>artist</td><td></td></tr></table>",
                "namespace",
            ),
            ("<table><tr><td>artist:</td></tr></table>", "values"),
            (
                "<table><tr><td>artist:</td><td><div class='bad'><span></span>tag</div></td></tr></table>",
                "power",
            ),
            (
                "<table><tr><td>artist:</td><td><div class='gt'><span></span>&invalid;</div></td></tr></table>",
                "entity",
            ),
        ];
        for (html, expected) in cases {
            let error = parse_one(html, TagGrammar::Detail)
                .expect_err("malformed shared tag markup should fail");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    fn parse_one(
        html: &str,
        grammar: TagGrammar,
    ) -> Result<eh_domain::contracts::GalleryTagGroup, RustError> {
        let Ok(dom) = tl::parse(html, ParserOptions::default()) else {
            return Err(RustError::Error("test tag HTML is invalid".to_owned()));
        };
        let Some(mut rows) = dom.query_selector("tr") else {
            return Err(RustError::Error("test tag row is missing".to_owned()));
        };
        let Some(row) = rows.next() else {
            return Err(RustError::Error("test tag row is missing".to_owned()));
        };
        parse_group(row, dom.parser(), grammar)
    }
}
