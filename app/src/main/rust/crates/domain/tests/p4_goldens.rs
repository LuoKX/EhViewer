//! P4 V1 fixtures consumed verbatim by Kotlin transport tests.
use eh_domain::comic_info_policy::{
    self, ComicInfoMetadataState, ComicInfoOperation, ComicInfoPolicyRequest,
};
use eh_domain::{
    bbcode::{self, BbCodeOperation, BbCodeRequest},
    text_spans::TextSpan,
};
use serde::{Serialize, de::DeserializeOwned};
use std::{fmt::Write as _, path::Path};
fn fixture<T: Serialize + DeserializeOwned + PartialEq>(name: &str, value: &T) {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let hex = bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    }) + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(format!("{name}.cbor.hex"));
    if std::env::var_os("UPDATE_P4_GOLDENS").is_some() {
        std::fs::write(&path, &hex).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), hex);
    assert!(serde_cbor::from_slice::<T>(&bytes).unwrap() == *value);
}
#[test]
fn exact_shared_bbcode_requests_and_responses() {
    let export = BbCodeRequest {
        version: 1,
        operation: BbCodeOperation::Export,
        units: vec![0x4e2d, 0xd83d, 0xde00, 0xd800, 120],
        origin_units: None,
        spans: vec![
            TextSpan {
                start: 0,
                end: 5,
                flags: 3,
            },
            TextSpan {
                start: 1,
                end: 3,
                flags: 2,
            },
        ],
    };
    fixture("bbcode_export_request", &export);
    fixture("bbcode_export_response", &bbcode::build_envelope(&export));
    let edit = BbCodeRequest {
        version: 1,
        operation: BbCodeOperation::Edit,
        units: vec![0xd800, 98, 97, 0xdc00],
        origin_units: Some(vec![0xd800, 97, 0xdc00]),
        spans: vec![],
    };
    fixture("bbcode_edit_request", &edit);
    fixture("bbcode_edit_response", &bbcode::build_envelope(&edit));
}

#[test]
fn exact_shared_comic_info_policy_requests_and_responses() {
    let prepare = ComicInfoPolicyRequest {
        version: 1,
        operation: ComicInfoOperation::Prepare,
        has_directory: true,
        detailed_gallery: false,
        fetch_metadata: false,
        gallery_pages: 0,
        expected_pages: Some(2),
        metadata_state: ComicInfoMetadataState::Unread,
        metadata_pages: None,
    };
    let mut cases = vec![("read", prepare.clone())];
    let mut create = prepare.clone();
    create.metadata_state = ComicInfoMetadataState::Missing;
    cases.push(("create", create.clone()));
    create.detailed_gallery = true;
    cases.push(("detailed", create));
    for (name, pages) in [("rewrite", 1), ("keep", 2)] {
        let mut value = prepare.clone();
        value.metadata_state = ComicInfoMetadataState::Present;
        value.metadata_pages = Some(pages);
        cases.push((name, value));
    }
    let mut resolve = prepare.clone();
    resolve.operation = ComicInfoOperation::ResolveExport;
    resolve.expected_pages = None;
    cases.push(("read_export", resolve.clone()));
    resolve.gallery_pages = 5;
    resolve.has_directory = false;
    cases.push(("resolve_known", resolve.clone()));
    resolve.gallery_pages = 0;
    resolve.has_directory = true;
    resolve.metadata_state = ComicInfoMetadataState::Present;
    resolve.metadata_pages = Some(7);
    cases.push(("resolve_metadata", resolve));
    let mut write = prepare;
    write.operation = ComicInfoOperation::Write;
    write.expected_pages = None;
    write.fetch_metadata = true;
    cases.push(("write", write.clone()));
    write.has_directory = false;
    cases.push(("skip", write));
    let mut invalid = cases[0].1.clone();
    invalid.expected_pages = Some(0);
    cases.push(("invalid_pages", invalid));
    let mut missing_dir = cases[0].1.clone();
    missing_dir.has_directory = false;
    cases.push(("missing_dir", missing_dir));
    let mut export_missing = cases[0].1.clone();
    export_missing.operation = ComicInfoOperation::ResolveExport;
    export_missing.expected_pages = None;
    export_missing.metadata_state = ComicInfoMetadataState::Missing;
    cases.push(("export_missing", export_missing));
    for (name, request) in cases {
        fixture(&format!("comic_info_policy_{name}_request"), &request);
        fixture(
            &format!("comic_info_policy_{name}_response"),
            &comic_info_policy::build_envelope(&request),
        );
    }
}
