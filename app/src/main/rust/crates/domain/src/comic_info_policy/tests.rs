use super::*;

fn request(operation: ComicInfoOperation) -> ComicInfoPolicyRequest {
    ComicInfoPolicyRequest {
        version: 1,
        operation,
        has_directory: true,
        detailed_gallery: false,
        fetch_metadata: true,
        gallery_pages: 0,
        expected_pages: if operation == ComicInfoOperation::Prepare {
            Some(2)
        } else {
            None
        },
        metadata_state: ComicInfoMetadataState::Unread,
        metadata_pages: None,
    }
}
fn observations() -> Vec<(ComicInfoMetadataState, Option<i32>)> {
    let mut values = vec![
        (ComicInfoMetadataState::Unread, None),
        (ComicInfoMetadataState::Missing, None),
    ];
    values.extend(
        [i32::MIN, -1, 0, 1, 2, i32::MAX]
            .map(|pages| (ComicInfoMetadataState::Present, Some(pages))),
    );
    values
}
fn payload(request: &ComicInfoPolicyRequest) -> ComicInfoPolicyPlan {
    build_envelope(request).payload.unwrap()
}
fn failure(request: &ComicInfoPolicyRequest, message: &str) {
    let result = build_envelope(request);
    assert!(result.payload.is_none());
    assert_eq!(result.error, Some(error(message)));
}

#[test]
fn preparation_matches_frozen_presence_page_and_detail_branches() {
    for pages in [i32::MIN, -1, 0, 1, 2, i32::MAX] {
        for has_directory in [false, true] {
            for detailed_gallery in [false, true] {
                for (metadata_state, metadata_pages) in observations() {
                    let mut r = request(ComicInfoOperation::Prepare);
                    r.expected_pages = Some(pages);
                    r.has_directory = has_directory;
                    r.detailed_gallery = detailed_gallery;
                    r.metadata_state = metadata_state;
                    r.metadata_pages = metadata_pages;
                    if pages <= 0 {
                        failure(&r, INVALID_PAGES);
                        continue;
                    }
                    if !has_directory {
                        failure(&r, MISSING_DIRECTORY);
                        continue;
                    }
                    let expected = match metadata_state {
                        ComicInfoMetadataState::Unread => plan(
                            ComicInfoAction::ReadMetadata,
                            None,
                            ComicInfoFetchMode::None,
                        ),
                        ComicInfoMetadataState::Missing => plan(
                            ComicInfoAction::Create,
                            Some(pages),
                            if detailed_gallery {
                                ComicInfoFetchMode::None
                            } else {
                                ComicInfoFetchMode::Continue
                            },
                        ),
                        ComicInfoMetadataState::Present => plan(
                            if metadata_pages == Some(pages) {
                                ComicInfoAction::Keep
                            } else {
                                ComicInfoAction::Rewrite
                            },
                            Some(pages),
                            ComicInfoFetchMode::None,
                        ),
                    };
                    assert_eq!(payload(&r), expected);
                }
            }
        }
    }
}
#[test]
fn export_prefers_gallery_pages_and_only_reads_xml_when_needed() {
    for gallery_pages in [i32::MIN, -1, 0, 1, 2, i32::MAX] {
        for has_directory in [false, true] {
            for (metadata_state, metadata_pages) in observations() {
                let mut r = request(ComicInfoOperation::ResolveExport);
                r.gallery_pages = gallery_pages;
                r.has_directory = has_directory;
                r.metadata_state = metadata_state;
                r.metadata_pages = metadata_pages;
                if gallery_pages > 0 {
                    assert_eq!(
                        payload(&r),
                        plan(
                            ComicInfoAction::ResolvePages,
                            Some(gallery_pages),
                            ComicInfoFetchMode::None
                        )
                    );
                } else if !has_directory {
                    failure(&r, INVALID_PAGES);
                } else if metadata_state == ComicInfoMetadataState::Unread {
                    assert_eq!(
                        payload(&r),
                        plan(
                            ComicInfoAction::ReadMetadata,
                            None,
                            ComicInfoFetchMode::None
                        )
                    );
                } else if let Some(pages) = metadata_pages.filter(|&pages| pages > 0) {
                    assert_eq!(
                        payload(&r),
                        plan(
                            ComicInfoAction::ResolvePages,
                            Some(pages),
                            ComicInfoFetchMode::None
                        )
                    );
                } else {
                    failure(&r, INVALID_PAGES);
                }
            }
        }
    }
}
#[test]
fn direct_write_preserves_refresh_failure_policy_and_post_fetch_page_observation() {
    for gallery_pages in [i32::MIN, 0, 1, i32::MAX] {
        for has_directory in [false, true] {
            for detailed_gallery in [false, true] {
                for fetch_metadata in [false, true] {
                    let mut r = request(ComicInfoOperation::Write);
                    r.gallery_pages = gallery_pages;
                    r.has_directory = has_directory;
                    r.detailed_gallery = detailed_gallery;
                    r.fetch_metadata = fetch_metadata;
                    assert_eq!(
                        payload(&r),
                        plan(
                            if has_directory {
                                ComicInfoAction::Create
                            } else {
                                ComicInfoAction::Skip
                            },
                            None,
                            if has_directory && !detailed_gallery && fetch_metadata {
                                ComicInfoFetchMode::Propagate
                            } else {
                                ComicInfoFetchMode::None
                            }
                        )
                    );
                }
            }
        }
    }
}
#[test]
fn malformed_observations_and_versions_have_fixed_input_free_errors() {
    for operation in [
        ComicInfoOperation::Prepare,
        ComicInfoOperation::ResolveExport,
        ComicInfoOperation::Write,
    ] {
        let base = request(operation);
        let mut wrong = base.clone();
        wrong.version = 2;
        failure(&wrong, "Invalid ComicInfo policy input");
        for (state, pages) in [
            (ComicInfoMetadataState::Unread, Some(1)),
            (ComicInfoMetadataState::Missing, Some(0)),
            (ComicInfoMetadataState::Present, None),
        ] {
            let mut wrong = base.clone();
            wrong.metadata_state = state;
            wrong.metadata_pages = pages;
            failure(&wrong, "Invalid ComicInfo policy input");
        }
        let mut wrong = base;
        wrong.expected_pages = if operation == ComicInfoOperation::Prepare {
            None
        } else {
            Some(1)
        };
        failure(&wrong, "Invalid ComicInfo policy input");
    }
}
#[test]
fn requests_and_plans_round_trip_all_wire_enums() {
    for operation in [
        ComicInfoOperation::Prepare,
        ComicInfoOperation::ResolveExport,
        ComicInfoOperation::Write,
    ] {
        for (metadata_state, metadata_pages) in observations() {
            let mut r = request(operation);
            r.metadata_state = metadata_state;
            r.metadata_pages = metadata_pages;
            let bytes = serde_cbor::to_vec(&r).unwrap();
            assert_eq!(
                serde_cbor::from_slice::<ComicInfoPolicyRequest>(&bytes).unwrap(),
                r
            );
            let response = build_envelope(&r);
            let bytes = serde_cbor::to_vec(&response).unwrap();
            assert_eq!(
                serde_cbor::from_slice::<ApiEnvelope<ComicInfoPolicyPlan>>(&bytes).unwrap(),
                response
            );
        }
    }
}
