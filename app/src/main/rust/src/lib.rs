pub mod contracts;
mod ffi;
pub mod parser;

use serde::{Serialize, Serializer, ser::SerializeTuple};
use std::fmt::{Debug, Display, Formatter};
use tl::{HTMLTag, Node, NodeHandle, Parser, VDom};

#[macro_export]
macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: std::sync::OnceLock<regex_lite::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| regex_lite::Regex::new($re).unwrap())
    }};
}

fn get_vdom_first_element_by_class_name<'a>(dom: &'a VDom, name: &str) -> Option<&'a Node<'a>> {
    let handle = dom.get_elements_by_class_name(name).next()?;
    handle.get(dom.parser())
}

fn get_first_element_by_class_name<'a>(
    node: &'a Node,
    parser: &'a Parser,
    id: &str,
) -> Option<&'a Node<'a>> {
    let handle = node.find_node(parser, &mut |n| match n.as_tag() {
        None => false,
        Some(tag) => tag.attributes().is_class_member(id),
    })?;
    handle.get(parser)
}

// tl does not support selecting child elements
// Workaround for https://github.com/y21/tl/issues/22
fn select_first<'a>(tag: &'a HTMLTag, parser: &'a Parser, id: &str) -> Option<&'a HTMLTag<'a>> {
    for n in tag.children().top().iter() {
        let tag = n.get(parser).and_then(Node::as_tag);
        if tag.is_some_and(|t| t.name() == id) {
            return tag;
        }
    }
    None
}

fn get_first_child<'a>(tag: &'a HTMLTag, parser: &'a Parser) -> Option<&'a HTMLTag<'a>> {
    tag.children()
        .top()
        .iter()
        .find_map(|n| n.get(parser)?.as_tag())
}

#[derive(Debug)]
enum EhError {
    NoHits,
    NoWatched,
    NeedLogin,
    NoHathClient,
    InsufficientFunds,
    GalleryUnavailable(String),
    IpBanned(String),
    Error(String),
}

impl EhError {
    fn variant_name(&self) -> &'static str {
        match self {
            EhError::NoHits => "NoHits",
            EhError::NoWatched => "NoWatched",
            EhError::NeedLogin => "NeedLogin",
            EhError::NoHathClient => "NoHathClient",
            EhError::InsufficientFunds => "InsufficientFunds",
            EhError::GalleryUnavailable(_) => "GalleryUnavailable",
            EhError::IpBanned(_) => "IpBanned",
            EhError::Error(_) => "Error",
        }
    }
}

impl Serialize for EhError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tup = serializer.serialize_tuple(2)?;
        tup.serialize_element(self.variant_name())?;
        let payload = match self {
            EhError::GalleryUnavailable(s) | EhError::IpBanned(s) | EhError::Error(s) => {
                std::slice::from_ref(s)
            }
            _ => &[],
        };
        tup.serialize_element(payload)?;
        tup.end()
    }
}

impl Display for EhError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self, f)
    }
}

impl std::error::Error for EhError {}

fn get_node_handle_attr<'a>(
    node: &NodeHandle,
    parser: &'a Parser,
    attr: &'a str,
) -> Option<&'a str> {
    get_node_attr(node.get(parser)?, attr)
}

fn get_node_attr<'a>(node: &'a Node<'_>, attr: &'a str) -> Option<&'a str> {
    get_tag_attr(node.as_tag()?, attr)
}

fn get_tag_attr<'a>(tag: &'a HTMLTag<'_>, attr: &'a str) -> Option<&'a str> {
    tag.attributes().get(attr)??.try_as_utf8_str()
}
