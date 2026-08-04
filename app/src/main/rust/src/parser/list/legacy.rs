//! Frozen gallery-list parser used only for runtime rollback.
//!
//! Keep behavior stable. New parsing behavior belongs in
//! `eh_net::gallery::list`; this module is deleted after the versioned path has
//! completed Android rollout and rollback rehearsal.

use std::borrow::Cow;
use std::ops::Index;

use anyhow::{Context, Result, bail};
use tl::{Bytes, Node, Parser, VDom};

use super::{
    BaseGalleryInfo, GalleryListResult, get_thumb_key, parse_token_and_gid, to_category_i32,
};
use crate::EhError;
use crate::parser::utils::unescape;
use crate::{
    get_first_element_by_class_name, get_node_attr, get_node_handle_attr,
    get_vdom_first_element_by_class_name, regex,
};

fn child_by_id<'a, S>(node: &'a Node<'_>, parser: &'a Parser, id: S) -> Option<&'a Node<'a>>
where
    S: Into<Bytes<'a>>,
{
    let id = id.into();
    node.find_node(parser, &mut |candidate| {
        candidate
            .as_tag()
            .and_then(|tag| tag.attributes().id())
            .is_some_and(|candidate_id| candidate_id.eq(&id))
    })?
    .get(parser)
}

fn first_descendant_attr<'a>(
    node: &'a Node<'_>,
    parser: &'a Parser,
    attribute: &'a str,
) -> Option<&'a str> {
    let selector = format!("[{attribute}]");
    let mut matches = node.as_tag()?.query_selector(parser, &selector)?;
    get_node_handle_attr(&matches.next()?, parser, attribute)
}

fn parse_rating(style: &str) -> f32 {
    let mut values = regex!("\\d+px").find_iter(style);
    let mut next = || {
        values
            .next()?
            .as_str()
            .trim_end_matches("px")
            .parse::<i32>()
            .ok()
    };
    match (next(), next()) {
        (Some(horizontal), Some(vertical)) => {
            let rating = 5 - horizontal / 16;
            if vertical == 21 {
                (rating - 1) as f32 + 0.5
            } else {
                rating as f32
            }
        }
        _ => -1.0,
    }
}

fn parse_uploader_and_pages(html: &str) -> (Option<String>, bool, i32) {
    let uploader =
        regex!(r#"<a href="https://e[x-]hentai.org/uploader/.*?">(.*?)</a>|(\(Disowned\))"#)
            .captures(html)
            .and_then(|captures| captures.get(1).or_else(|| captures.get(2)))
            .map(|value| {
                let value = value.as_str();
                unescape(value).as_deref().unwrap_or(value).to_owned()
            });
    let pages = regex!(r"<div>(\d+) pages</div>")
        .captures(html)
        .and_then(|captures| captures[1].parse().ok())
        .unwrap_or(0);
    (uploader, html.contains("style=\"opacity:0.5\""), pages)
}

fn parse_thumbnail_resolution(style: &str) -> (i32, i32) {
    regex!(r"height:(\d+)px;width:(\d+)px")
        .captures(style)
        .map_or((0, 0), |captures| {
            (
                captures[1].parse().unwrap_or(0),
                captures[2].parse().unwrap_or(0),
            )
        })
}

fn parse_gallery_info(node: &Node<'_>, parser: &Parser) -> Option<BaseGalleryInfo> {
    let tag = node.as_tag()?;
    let title = get_first_element_by_class_name(node, parser, "glink")?.inner_text(parser);
    let name = get_first_element_by_class_name(node, parser, "glname")?;
    let detail_url = first_descendant_attr(name, parser, "href")
        .or_else(|| first_descendant_attr(node, parser, "href"))?;
    let (gallery_id, token) = parse_token_and_gid(detail_url)?;
    let simple_tags = tag
        .query_selector(parser, ".gt, .gtl")?
        .filter_map(|handle| get_node_handle_attr(&handle, parser, "title").map(str::to_owned))
        .collect();
    let (thumbnail, (thumbnail_height, thumbnail_width)) =
        match tag.query_selector(parser, "[data-src]")?.next() {
            None => {
                let thumbnail = tag.query_selector(parser, "[src]")?.next()?;
                (
                    get_node_handle_attr(&thumbnail, parser, "src")?,
                    parse_thumbnail_resolution(get_node_handle_attr(&thumbnail, parser, "style")?),
                )
            }
            Some(thumbnail) => (
                get_node_handle_attr(&thumbnail, parser, "data-src")?,
                parse_thumbnail_resolution(get_node_handle_attr(&thumbnail, parser, "style")?),
            ),
        };
    let category = get_first_element_by_class_name(node, parser, "cn")
        .or_else(|| get_first_element_by_class_name(node, parser, "cs"))
        .map_or_else(
            || Cow::Borrowed("unknown"),
            |value| value.inner_text(parser),
        );
    let posted_id = format!("posted_{gallery_id}");
    let (posted, favorite_name) = child_by_id(node, parser, posted_id.as_str()).map_or_else(
        || (String::new(), None),
        |posted_node| {
            (
                posted_node.inner_text(parser).trim().to_owned(),
                get_node_attr(posted_node, "title")
                    .map(|value| unescape(value).as_deref().unwrap_or(value).to_owned()),
            )
        },
    );
    let rating_node = get_first_element_by_class_name(node, parser, "ir")?
        .as_tag()?
        .attributes();
    let rating_class = rating_node.class()?.try_as_utf8_str()?;
    let rating_style = rating_node.get("style")??.try_as_utf8_str()?;
    let (uploader, disowned, pages) = tag
        .query_selector(parser, ".glhide, .gl3e, .gl5t")?
        .next()
        .and_then(|handle| handle.get(parser))
        .map_or((None, false, 0), |details| {
            parse_uploader_and_pages(&details.inner_html(parser))
        });
    let favorite_note_id = format!("favnote_{gallery_id}");
    let favorite_note = child_by_id(node, parser, favorite_note_id.as_str()).map(|value| {
        let text = value.inner_text(parser);
        unescape(&text).as_deref().unwrap_or(&text).to_owned()
    });

    Some(BaseGalleryInfo {
        gid: gallery_id,
        token,
        title: unescape(title.trim()).ok()?.into_owned(),
        titleJpn: None,
        thumbKey: get_thumb_key(thumbnail),
        category: to_category_i32(&category.trim().to_lowercase()),
        posted,
        uploader,
        disowned,
        rating: parse_rating(rating_style),
        rated: rating_class.contains("irr")
            || rating_class.contains("irg")
            || rating_class.contains("irb"),
        simpleTags: simple_tags,
        pages,
        thumbWidth: thumbnail_width,
        thumbHeight: thumbnail_height,
        simpleLanguage: None,
        favoriteSlot: if favorite_name.is_some() { 0 } else { -2 },
        favoriteName: favorite_name,
        favoriteNote: favorite_note,
    })
}

pub(super) fn parse_info_list(dom: &VDom, parser: &Parser) -> Result<GalleryListResult> {
    if let Some(warning) = get_vdom_first_element_by_class_name(dom, "searchwarn") {
        let text = warning.inner_text(parser);
        let error = if text.contains("You do not have any watched tags") {
            EhError::NoWatched
        } else {
            EhError::Error(text.into_owned())
        };
        bail!(error);
    }
    let gallery_info_list = get_vdom_first_element_by_class_name(dom, "itg")
        .and_then(Node::children)
        .map(|children| {
            children
                .top()
                .iter()
                .filter_map(|handle| parse_gallery_info(handle.get(parser)?, parser))
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .context(EhError::NoHits)?;
    let prev = dom.get_element_by_id("uprev").and_then(|handle| {
        let href = get_node_handle_attr(&handle, parser, "href")?;
        Some(
            regex!("prev=(\\d+(-\\d+)?)")
                .captures(href)?
                .index(1)
                .to_owned(),
        )
    });
    let next = dom.get_element_by_id("unext").and_then(|handle| {
        let href = get_node_handle_attr(&handle, parser, "href")?;
        Some(
            regex!("next=(\\d+(-\\d+)?)")
                .captures(href)?
                .index(1)
                .to_owned(),
        )
    });
    Ok(GalleryListResult {
        prev,
        next,
        galleryInfoList: gallery_info_list,
    })
}
