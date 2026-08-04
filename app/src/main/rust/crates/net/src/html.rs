use tl::{Bytes, HTMLTag, Node, NodeHandle, Parser, VDom};

pub(crate) fn first_by_class<'a>(dom: &'a VDom<'_>, name: &str) -> Option<&'a Node<'a>> {
    dom.get_elements_by_class_name(name)
        .next()?
        .get(dom.parser())
}

pub(crate) fn first_child_by_class<'a>(
    node: &'a Node<'_>,
    parser: &'a Parser<'_>,
    name: &str,
) -> Option<&'a Node<'a>> {
    node.find_node(parser, &mut |candidate| {
        candidate
            .as_tag()
            .is_some_and(|tag| tag.attributes().is_class_member(name))
    })?
    .get(parser)
}

pub(crate) fn first_element_child<'a>(
    tag: &'a HTMLTag<'_>,
    parser: &'a Parser<'_>,
) -> Option<&'a HTMLTag<'a>> {
    tag.children()
        .top()
        .iter()
        .find_map(|handle| handle.get(parser).and_then(Node::as_tag))
}

pub(crate) fn child_by_id<'a>(
    node: &'a Node<'_>,
    parser: &'a Parser<'_>,
    id: &str,
) -> Option<&'a Node<'a>> {
    let id = Bytes::from(id);
    node.find_node(parser, &mut |candidate| {
        candidate
            .as_tag()
            .and_then(|tag| tag.attributes().id())
            .is_some_and(|candidate_id| candidate_id.eq(&id))
    })?
    .get(parser)
}

pub(crate) fn first_descendant_attr<'a>(
    node: &'a Node<'_>,
    parser: &'a Parser<'_>,
    attribute: &'a str,
) -> Option<&'a str> {
    let selector = format!("[{attribute}]");
    let mut matches = node.as_tag()?.query_selector(parser, &selector)?;
    handle_attr(matches.next()?, parser, attribute)
}

pub(crate) fn handle_attr<'a>(
    handle: NodeHandle,
    parser: &'a Parser<'_>,
    attribute: &'a str,
) -> Option<&'a str> {
    node_attr(handle.get(parser)?, attribute)
}

pub(crate) fn node_attr<'a>(node: &'a Node<'_>, attribute: &'a str) -> Option<&'a str> {
    node.as_tag()?
        .attributes()
        .get(attribute)??
        .try_as_utf8_str()
}
