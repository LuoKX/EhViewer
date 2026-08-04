//! Gallery-comment response parsing.

use std::borrow::Cow;

use chrono::NaiveDateTime;
use eh_domain::contracts::{ApiEnvelope, GalleryComment, GalleryCommentsPage, RustError};
use tl::{HTMLTag, Node, NodeHandle, Parser, ParserOptions, VDom};

use crate::html::{first_child_by_class, first_element_child, node_attr};
use crate::text::unescape;

/// Current gallery-comments bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module discriminator used by the Android bridge.
pub const MODULE: &str = "galleryComments";

/// Parses gallery-comment HTML and always returns a versioned success/error
/// envelope suitable for the V1 JNI boundary.
#[must_use]
pub fn parse_envelope(html: &str) -> ApiEnvelope<GalleryCommentsPage> {
    match parse(html) {
        Ok(page) => ApiEnvelope::ok(API_VERSION, MODULE, page),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, error),
    }
}

/// Parses gallery comments and their current-account interaction state.
///
/// # Errors
///
/// Returns [`RustError::IpBanned`] when the response is not HTML and
/// [`RustError::Error`] when required comment markup, dates, identifiers, or
/// escaped text are malformed. A remote warning immediately after the comment
/// header is preserved as [`RustError::Error`].
pub fn parse(html: &str) -> Result<GalleryCommentsPage, RustError> {
    let options = ParserOptions::default().track_ids();
    let mut dom = tl::parse(html, options)
        .map_err(|error| RustError::Error(format!("invalid gallery-comments HTML: {error}")))?;
    if dom.version().is_none() {
        return Err(RustError::IpBanned(html.to_owned()));
    }
    parse_dom(&mut dom)
}

pub(crate) fn parse_dom(dom: &mut VDom<'_>) -> Result<GalleryCommentsPage, RustError> {
    let (comment_handles, has_more) = {
        let parser = dom.parser();
        let comment_container = dom
            .get_element_by_id("cdiv")
            .and_then(|handle| handle.get(parser))
            .and_then(Node::as_tag)
            .ok_or_else(|| RustError::Error("gallery comment container is missing".to_owned()))?;
        let header_handle = dom
            .get_element_by_id("chd")
            .ok_or_else(|| RustError::Error("gallery comment header is missing".to_owned()))?;
        let children = comment_container.children();
        let child_handles = children.top().as_slice();
        let header_index = child_handles
            .iter()
            .rposition(|handle| *handle == header_handle)
            .ok_or_else(|| {
                RustError::Error("gallery comment header is not a direct child".to_owned())
            })?;

        if let Some(warning) = child_handles
            .iter()
            .skip(header_index + 1)
            .find_map(|handle| handle.get(parser).and_then(Node::as_tag))
            .filter(|tag| tag.name() == "p")
        {
            return Err(RustError::Error(warning.inner_text(parser).into_owned()));
        }

        let has_more = header_handle
            .get(parser)
            .and_then(Node::as_tag)
            .and_then(|header| first_element_child(header, parser))
            .is_some_and(|control| {
                control
                    .attributes()
                    .id()
                    .and_then(|id| id.try_as_utf8_str())
                    != Some("postnewcomment")
            });
        let comment_handles = comment_container
            .query_selector(parser, ".c1")
            .ok_or(RustError::Error("gallery comment query failed".to_owned()))?
            .collect::<Vec<_>>();
        (comment_handles, has_more)
    };

    let comments = comment_handles
        .into_iter()
        .map(|handle| parse_comment(dom, handle))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GalleryCommentsPage { comments, has_more })
}

fn parse_comment(dom: &mut VDom<'_>, handle: NodeHandle) -> Result<GalleryComment, RustError> {
    let (mut comment, body_handle, underline_handles) = {
        let parser = dom.parser();
        let node = handle.get(parser).ok_or(RustError::Error(
            "gallery comment entry is missing".to_owned(),
        ))?;
        let mut comment = parse_comment_header(node, parser)?;
        apply_comment_controls(&mut comment, node, parser);
        parse_comment_metadata(&mut comment, node, parser)?;
        let (body_handle, underline_handles) = find_comment_body(node, parser)?;
        (comment, body_handle, underline_handles)
    };

    for underline_handle in underline_handles {
        let underline = underline_handle
            .get_mut(dom.parser_mut())
            .and_then(Node::as_tag_mut)
            .ok_or(RustError::Error(
                "gallery comment underline is malformed".to_owned(),
            ))?;
        *underline.name_mut() = "u".into();
        underline.attributes_mut().remove("style");
    }

    let body_node = body_handle.get(dom.parser()).ok_or(RustError::Error(
        "gallery comment body disappeared".to_owned(),
    ))?;
    let body = body_node.as_tag().ok_or(RustError::Error(
        "gallery comment body is malformed".to_owned(),
    ))?;
    comment.id = node_attr(body_node, "id")
        .and_then(|id| id.strip_prefix("comment_"))
        .and_then(|id| id.parse::<i64>().ok())
        .filter(|id| *id >= 0)
        .ok_or_else(|| RustError::Error("gallery comment identifier is malformed".to_owned()))?;
    comment.uploader = comment.id == 0;
    comment.body_html = body.inner_html(dom.parser());
    Ok(comment)
}

fn parse_comment_header(node: &Node<'_>, parser: &Parser<'_>) -> Result<GalleryComment, RustError> {
    let header = first_child_by_class(node, parser, "c3")
        .and_then(Node::as_tag)
        .ok_or_else(|| RustError::Error("gallery comment header is missing".to_owned()))?;
    let header_children = header.children();
    let posted_text = header_children
        .top()
        .as_slice()
        .first()
        .and_then(|child| child.get(parser))
        .map(|child| child.inner_text(parser))
        .ok_or_else(|| RustError::Error("gallery comment posted date is missing".to_owned()))?;

    Ok(GalleryComment {
        posted_at: parse_comment_time(&posted_text, "Posted on ").ok_or_else(|| {
            RustError::Error("gallery comment posted date is malformed".to_owned())
        })?,
        user: first_element_child(header, parser)
            .map(|author| {
                unescaped_inner_text(
                    author,
                    parser,
                    "gallery comment author contains an invalid entity",
                )
            })
            .transpose()?,
        ..GalleryComment::default()
    })
}

fn apply_comment_controls(comment: &mut GalleryComment, node: &Node<'_>, parser: &Parser<'_>) {
    let Some(controls) = first_child_by_class(node, parser, "c4").and_then(Node::as_tag) else {
        return;
    };
    let control_children = controls.children();
    for control_handle in control_children.top().iter() {
        let Some(control_node) = control_handle.get(parser) else {
            continue;
        };
        let Some(control) = control_node.as_tag() else {
            continue;
        };
        match control.inner_text(parser).as_ref() {
            "Vote+" => {
                comment.can_vote_up = true;
                comment.voted_up =
                    node_attr(control_node, "style").is_some_and(|style| !style.is_empty());
            }
            "Vote-" => {
                comment.can_vote_down = true;
                comment.voted_down =
                    node_attr(control_node, "style").is_some_and(|style| !style.is_empty());
            }
            "Edit" => comment.editable = true,
            _ => {}
        }
    }
}

fn parse_comment_metadata(
    comment: &mut GalleryComment,
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> Result<(), RustError> {
    comment.score = first_child_by_class(node, parser, "c5")
        .and_then(Node::as_tag)
        .and_then(|score| first_element_child(score, parser))
        .and_then(|score| score.inner_text(parser).parse::<i32>().ok())
        .unwrap_or(0);
    comment.vote_state = first_child_by_class(node, parser, "c7")
        .and_then(Node::as_tag)
        .map(|state| {
            unescaped_inner_text(
                state,
                parser,
                "gallery comment vote state contains an invalid entity",
            )
        })
        .transpose()?;
    comment.last_edited_at = first_child_by_class(node, parser, "c8")
        .and_then(|edited| parse_comment_time(&edited.inner_text(parser), "Last edited on "))
        .unwrap_or(0);
    Ok(())
}

fn find_comment_body(
    node: &Node<'_>,
    parser: &Parser<'_>,
) -> Result<(NodeHandle, Vec<NodeHandle>), RustError> {
    let body_handle = node
        .find_node(parser, &mut |candidate| {
            candidate
                .as_tag()
                .is_some_and(|tag| tag.attributes().is_class_member("c6"))
        })
        .ok_or_else(|| RustError::Error("gallery comment body is missing".to_owned()))?;
    let body = body_handle
        .get(parser)
        .and_then(Node::as_tag)
        .ok_or(RustError::Error(
            "gallery comment body is malformed".to_owned(),
        ))?;
    let underline_handles = body
        .query_selector(parser, "span[style='text-decoration:underline;']")
        .ok_or(RustError::Error(
            "gallery comment underline query failed".to_owned(),
        ))?
        .collect::<Vec<_>>();
    Ok((body_handle, underline_handles))
}

fn unescaped_inner_text(
    tag: &HTMLTag<'_>,
    parser: &Parser<'_>,
    error_message: &str,
) -> Result<String, RustError> {
    let raw = tag.inner_text(parser);
    unescape(&raw)
        .map(Cow::into_owned)
        .map_err(|_| RustError::Error(error_message.to_owned()))
}

fn parse_comment_time(value: &str, prefix: &str) -> Option<i64> {
    value
        .strip_prefix(prefix)
        .and_then(|value| NaiveDateTime::parse_and_remainder(value, "%d %B %Y, %R").ok())
        .map(|(date_time, _)| date_time.and_utc().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use eh_domain::contracts::{ApiEnvelope, GalleryComment, GalleryCommentsPage, RustError};

    use super::{API_VERSION, MODULE, parse, parse_comment_time, parse_envelope};
    use crate::test_support::to_hex;

    const FULL_PAGE: &str = include_str!("../../test_data/gallery_comments.html");
    const WARNING_PAGE: &str = include_str!("../../test_data/gallery_comments_warning.html");
    const V1_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656f67616c6c657279436f6d6d656e7473677061796c6f",
        "6164a268636f6d6d656e747381ad626964187b6573636f726526686564697461626c65f56a766f74",
        "65557041626c65f568766f746555704564f56c766f7465446f776e41626c65f56a766f7465446f77",
        "6e4564f46875706c6f61646572f469766f746553746174656d566f7465642026206e6f7465646474",
        "696d651b0000019fc2b8836064757365726b416c696365202620426f6267636f6d6d656e74774669",
        "727374203c753e756e6465726c696e65643c2f753e6a6c6173744564697465641b0000019fc80812",
        "40676861734d6f7265f5656572726f72f6",
    );

    #[test]
    fn parses_complete_and_minimal_comments_from_offline_html() {
        let page = parse(FULL_PAGE).expect("the checked-in comments page should parse");
        assert!(page.has_more);
        assert_eq!(page.comments.len(), 2);

        let first = &page.comments[0];
        assert_eq!(first.id, 123);
        assert_eq!(first.score, -7);
        assert!(first.editable);
        assert!(first.can_vote_up);
        assert!(first.voted_up);
        assert!(first.can_vote_down);
        assert!(!first.voted_down);
        assert!(!first.uploader);
        assert_eq!(first.vote_state.as_deref(), Some("Voted & noted"));
        assert_eq!(first.posted_at, 1_785_678_300_000);
        assert_eq!(first.user.as_deref(), Some("Alice & Bob"));
        assert!(first.body_html.contains("<u>underlined</u>"));
        assert!(first.body_html.contains("&amp; safe"));
        assert_eq!(first.last_edited_at, 1_785_767_400_000);

        let second = &page.comments[1];
        assert_eq!(second.id, 0);
        assert_eq!(second.score, 0);
        assert!(second.uploader);
        assert!(!second.editable);
        assert!(!second.can_vote_up);
        assert_eq!(second.vote_state, None);
        assert_eq!(second.last_edited_at, 0);
        assert!(second.body_html.contains("<b>Uploader note</b>"));
    }

    #[test]
    fn parses_empty_comment_page_and_has_more_boundaries() {
        let no_more = parse(
            "<!doctype html><html><body><div id='cdiv'><div id='chd'><a id='postnewcomment'>Post</a></div></div></body></html>",
        )
        .expect("an empty comments page should parse");
        assert_eq!(no_more, GalleryCommentsPage::default());

        let no_control = parse(
            "<!doctype html><html><body><div id='cdiv'><div id='chd'></div></div></body></html>",
        )
        .expect("a header without a control should parse");
        assert!(!no_control.has_more);
        assert!(no_control.comments.is_empty());

        let unnamed_control = parse(
            "<!doctype html><html><body><div id='cdiv'><div id='chd'><a>More</a></div></div></body></html>",
        )
        .expect("an unnamed load-more control should preserve legacy behavior");
        assert!(unnamed_control.has_more);
    }

    #[test]
    fn returns_remote_warning_and_non_html_as_typed_errors() {
        assert_eq!(
            parse(WARNING_PAGE),
            Err(RustError::Error("Comment access denied".to_owned()))
        );
        assert_eq!(
            parse("blocked"),
            Err(RustError::IpBanned("blocked".to_owned()))
        );
    }

    #[test]
    fn rejects_missing_or_malformed_comment_markup() {
        let cases = [
            (
                "<!doctype html><html><body></body></html>",
                "container is missing",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'></div></body></html>",
                "header is missing",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div><div id='chd'></div></div></div></body></html>",
                "not a direct child",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c6' id='comment_1'>x</div></div></div></body></html>",
                "comment header is missing",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'></div><div class='c6' id='comment_1'>x</div></div></div></body></html>",
                "posted date is missing",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted yesterday</div><div class='c6' id='comment_1'>x</div></div></div></body></html>",
                "posted date is malformed",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted on 02 August 2026, 13:45</div></div></div></body></html>",
                "body is missing",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted on 02 August 2026, 13:45</div><div class='c6' id='bad'>x</div></div></div></body></html>",
                "identifier is malformed",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted on 02 August 2026, 13:45</div><div class='c6' id='comment_-1'>x</div></div></div></body></html>",
                "identifier is malformed",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted on 02 August 2026, 13:45<a>&unknown;</a></div><div class='c6' id='comment_1'>x</div></div></div></body></html>",
                "author contains an invalid entity",
            ),
            (
                "<!doctype html><html><body><div id='cdiv'><div id='chd'></div><div class='c1'><div class='c3'>Posted on 02 August 2026, 13:45</div><div class='c7'>&unknown;</div><div class='c6' id='comment_1'>x</div></div></div></body></html>",
                "vote state contains an invalid entity",
            ),
        ];

        for (html, expected) in cases {
            let error = parse(html).expect_err("malformed comment HTML must be rejected");
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn comment_dates_preserve_legacy_prefix_and_trailing_text_behavior() {
        assert_eq!(
            parse_comment_time("Posted on 02 August 2026, 13:45 by Alice", "Posted on "),
            Some(1_785_678_300_000)
        );
        assert_eq!(
            parse_comment_time("02 August 2026, 13:45", "Posted on "),
            None
        );
        assert_eq!(parse_comment_time("Posted on invalid", "Posted on "), None);
    }

    #[test]
    fn wraps_success_and_error_in_the_versioned_envelope() {
        let success = parse_envelope(FULL_PAGE);
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert_eq!(
            success.payload.as_ref().map(|page| page.comments.len()),
            Some(2)
        );
        assert!(success.is_ok());

        let failure = parse_envelope(WARNING_PAGE);
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(
            failure.error,
            Some(RustError::Error("Comment access denied".to_owned()))
        );
    }

    #[test]
    fn versioned_contract_matches_the_android_cbor_golden_fixture() {
        let envelope = ApiEnvelope::ok(
            API_VERSION,
            MODULE,
            GalleryCommentsPage {
                comments: vec![complete_contract_comment()],
                has_more: true,
            },
        );
        let encoded = serde_cbor::to_vec(&envelope).expect("V1 envelope should serialize");
        assert_eq!(to_hex(&encoded), V1_GOLDEN_CBOR_HEX);

        let decoded: ApiEnvelope<GalleryCommentsPage> =
            serde_cbor::from_slice(&encoded).expect("golden envelope should decode in Rust");
        assert_eq!(decoded, envelope);
    }

    fn complete_contract_comment() -> GalleryComment {
        GalleryComment {
            id: 123,
            score: -7,
            editable: true,
            can_vote_up: true,
            voted_up: true,
            can_vote_down: true,
            voted_down: false,
            uploader: false,
            vote_state: Some("Voted & noted".to_owned()),
            posted_at: 1_785_678_300_000,
            user: Some("Alice & Bob".to_owned()),
            body_html: "First <u>underlined</u>".to_owned(),
            last_edited_at: 1_785_767_400_000,
        }
    }
}
