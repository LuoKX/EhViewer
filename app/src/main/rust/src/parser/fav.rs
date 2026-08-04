use crate::parser::list::{GalleryListResult, parse_info_list};
use crate::parser::utils::unescape;
use anyhow::{Result, bail};
use serde::Serialize;
use tl::Parser;
use tl::VDom;

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct FavResult {
    catArray: Vec<String>,
    countArray: Vec<i32>,
    galleryListResult: GalleryListResult,
}

pub fn parse_fav(dom: &VDom, parser: &Parser) -> Result<FavResult> {
    let vec: Vec<(String, i32)> = dom
        .get_elements_by_class_name("fp")
        .enumerate()
        .filter_map(|(i, e)| {
            if i == 10 {
                return None;
            }
            let top = e.get(parser)?.children()?;
            let children = top.top();
            let cat = children[5].get(parser)?.inner_text(parser);
            let name = unescape(&cat).ok()?.trim().to_string();
            let count = children[1]
                .get(parser)?
                .inner_text(parser)
                .parse::<i32>()
                .ok()?;
            Some((name, count))
        })
        .collect();
    if vec.len() == 10 {
        let list = parse_info_list(dom, parser)?;
        let cat = vec.iter().cloned().unzip();
        Ok(FavResult {
            catArray: cat.0,
            countArray: cat.1,
            galleryListResult: list,
        })
    } else {
        bail!("Illegal fav cat count!")
    }
}

#[cfg(test)]
mod tests {
    use tl::ParserOptions;

    use super::parse_fav;

    const RESPONSE: &str = include_str!("../../crates/net/test_data/gallery_favorites.html");

    #[test]
    fn legacy_and_v1_favorites_parsers_share_the_checked_in_success_semantics() {
        let dom = tl::parse(RESPONSE, ParserOptions::default())
            .expect("checked-in Favorites HTML should parse");
        let legacy = parse_fav(&dom, dom.parser()).expect("legacy Favorites parser should succeed");
        let core = eh_net::gallery::favorites::parse(RESPONSE)
            .expect("workspace Favorites parser should succeed");

        assert_eq!(legacy.catArray, core.categories);
        assert_eq!(legacy.countArray, core.counts);
        assert_eq!(
            serde_cbor::value::to_value(legacy.galleryListResult)
                .expect("legacy Gallery List result should serialize"),
            serde_cbor::value::to_value(core.page)
                .expect("workspace Gallery List page should serialize")
        );
    }
}
