use super::*;

fn snapshot(
    pages: i32,
    tags_missing: bool,
    artists_missing: bool,
    has_directory: bool,
) -> Snapshot {
    Snapshot {
        pages,
        tags_missing,
        artists_missing,
        has_directory,
    }
}
fn request(entries: Vec<Snapshot>) -> Request {
    Request {
        version: 1,
        phase: Phase::Observe,
        entries,
        observations: Vec::new(),
    }
}
fn comic(penciller: Option<Vec<String>>) -> ComicFacts {
    ComicFacts {
        pages: -7,
        simple_tags: Some(vec!["group:g".into(), "artist:a".into()]),
        penciller,
    }
}
fn plan(request: &Request) -> Plan {
    build_envelope(request).payload.unwrap()
}
fn rejected(request: &Request) {
    let envelope = build_envelope(request);
    assert!(envelope.payload.is_none());
    assert_eq!(
        envelope.error,
        Some(RustError::Error("Invalid download metadata input".into()))
    );
}

/// Frozen legacy readMetadataFromLocal branches, never used by production.
fn legacy(entry: &Snapshot, fact: &Observation) -> Option<Patch> {
    if !entry.has_directory || !(entry.pages == 0 || entry.tags_missing || entry.artists_missing) {
        return None;
    }
    let update_gallery = entry.pages == 0 || entry.tags_missing;
    if let Some(comic) = &fact.comic {
        let artists = if entry.artists_missing {
            comic.penciller.clone()
        } else {
            None
        };
        if !update_gallery && artists.is_none() {
            return None;
        }
        Some(Patch {
            index: fact.index,
            pages: update_gallery.then_some(comic.pages),
            update_tags: update_gallery,
            simple_tags: if update_gallery {
                comic.simple_tags.clone()
            } else {
                None
            },
            persist_artists: artists.is_some(),
            artists,
            persist_gallery: update_gallery,
        })
    } else if entry.pages == 0 {
        fact.spider_pages.map(|pages| Patch {
            index: fact.index,
            pages: Some(pages),
            update_tags: false,
            simple_tags: None,
            artists: None,
            persist_gallery: true,
            persist_artists: false,
        })
    } else {
        None
    }
}

#[test]
fn all_missing_field_combinations_match_frozen_legacy_including_negative_pages() {
    for pages in [i32::MIN, -1, 0, 9, i32::MAX] {
        for tags_missing in [false, true] {
            for artists_missing in [false, true] {
                for has_directory in [false, true] {
                    let entry = snapshot(pages, tags_missing, artists_missing, has_directory);
                    let mut req = request(vec![entry.clone()]);
                    let reads = plan(&req).reads;
                    let expected_read =
                        has_directory && (pages == 0 || tags_missing || artists_missing);
                    assert_eq!(reads.len(), usize::from(expected_read));
                    if !expected_read {
                        continue;
                    }
                    assert_eq!(
                        reads[0],
                        ReadPlan {
                            index: 0,
                            spider_if_comic_missing: pages == 0
                        }
                    );
                    for penciller in [
                        None,
                        Some(Vec::new()),
                        Some(vec!["a".into(), "a".into(), "β".into()]),
                    ] {
                        for tags in [None, Some(Vec::new()), Some(vec!["group:g".into()])] {
                            let mut comic = comic(penciller.clone());
                            comic.simple_tags = tags;
                            let fact = Observation {
                                index: 0,
                                comic: Some(comic),
                                spider_pages: None,
                            };
                            req.phase = Phase::Patch;
                            req.observations = vec![fact.clone()];
                            assert_eq!(
                                plan(&req).patches,
                                legacy(&entry, &fact).into_iter().collect::<Vec<_>>()
                            );
                        }
                    }
                    for spider_pages in [None, Some(i32::MIN), Some(0), Some(7), Some(i32::MAX)] {
                        if pages != 0 && spider_pages.is_some() {
                            continue;
                        }
                        let fact = Observation {
                            index: 0,
                            comic: None,
                            spider_pages,
                        };
                        req.observations = vec![fact.clone()];
                        assert_eq!(
                            plan(&req).patches,
                            legacy(&entry, &fact).into_iter().collect::<Vec<_>>()
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn existing_comic_info_codec_supplies_tags_and_duplicate_artists_without_new_projection() {
    let decoded = crate::comic_info::decode_xml(b"<ComicInfo><Writer>g</Writer><Penciller>a, a</Penciller><PageCount>3</PageCount></ComicInfo>").unwrap();
    let mut req = request(vec![snapshot(0, true, true, true)]);
    req.phase = Phase::Patch;
    req.observations = vec![Observation {
        index: 0,
        comic: Some(ComicFacts {
            pages: decoded.page_count,
            simple_tags: decoded.simple_tags,
            penciller: decoded.penciller,
        }),
        spider_pages: None,
    }];
    let patch = plan(&req).patches.remove(0);
    assert_eq!(
        patch.simple_tags,
        Some(vec!["g".into(), "a".into(), "a".into()])
    );
    assert_eq!(patch.artists, Some(vec!["a".into(), "a".into()]));
}
#[test]
fn incomplete_complete_absent_directory_and_noop_observations_remain_stable() {
    let mut req = request(vec![
        snapshot(2, false, false, true),
        snapshot(0, true, true, false),
        snapshot(2, false, true, true),
        snapshot(0, false, false, true),
    ]);
    assert_eq!(
        plan(&req).reads,
        vec![
            ReadPlan {
                index: 2,
                spider_if_comic_missing: false
            },
            ReadPlan {
                index: 3,
                spider_if_comic_missing: true
            }
        ]
    );
    req.phase = Phase::Patch;
    req.observations = vec![
        Observation {
            index: 2,
            comic: Some(comic(None)),
            spider_pages: None,
        },
        Observation {
            index: 3,
            comic: None,
            spider_pages: None,
        },
    ];
    assert!(plan(&req).patches.is_empty());
    assert_eq!(
        plan(&request(Vec::new())),
        Plan {
            reads: Vec::new(),
            patches: Vec::new()
        }
    );
    req.entries.clear();
    req.observations.clear();
    assert_eq!(
        plan(&req),
        Plan {
            reads: Vec::new(),
            patches: Vec::new()
        }
    );
}
#[test]
fn malformed_version_counts_indices_and_priorities_are_closed_and_redacted() {
    let base = request(vec![snapshot(0, true, true, true)]);
    let mut wrong = base.clone();
    wrong.version = 2;
    rejected(&wrong);
    wrong = base.clone();
    wrong.entries = vec![base.entries[0].clone(); MAX_ROWS + 1];
    rejected(&wrong);
    wrong = base.clone();
    wrong.observations = vec![Observation {
        index: 0,
        comic: None,
        spider_pages: None,
    }];
    rejected(&wrong);
    wrong = base.clone();
    wrong.phase = Phase::Patch;
    rejected(&wrong);
    let fact = Observation {
        index: 0,
        comic: Some(comic(Some(vec!["secret".into()]))),
        spider_pages: Some(9),
    };
    wrong.observations = vec![fact.clone()];
    rejected(&wrong);
    wrong.observations = vec![Observation {
        index: 100,
        comic: None,
        spider_pages: None,
    }];
    rejected(&wrong);
    wrong.observations = vec![
        Observation {
            index: 0,
            comic: None,
            spider_pages: None
        };
        2
    ];
    rejected(&wrong);
    wrong.entries[0].pages = 2;
    wrong.observations = vec![Observation {
        index: 0,
        comic: None,
        spider_pages: Some(9),
    }];
    rejected(&wrong);
    assert!(serde_cbor::from_slice::<Request>(&[0xff]).is_err());
    assert!(
        serde_json::from_str::<Request>(
            r#"{"version":1,"phase":"secret-token","entries":[],"observations":[]}"#
        )
        .is_err()
    );
}
#[test]
fn text_and_string_budgets_have_exact_inclusive_limits() {
    let mut req = request(vec![snapshot(0, true, true, true); MAX_ROWS]);
    assert_eq!(plan(&req).reads.len(), MAX_ROWS);
    req.entries.truncate(1);
    req.phase = Phase::Patch;
    req.observations = vec![Observation {
        index: 0,
        comic: Some(ComicFacts {
            pages: 0,
            simple_tags: Some(vec!["é".repeat(MAX_TEXT_BYTES / 2)]),
            penciller: None,
        }),
        spider_pages: None,
    }];
    assert_eq!(plan(&req).patches.len(), 1);
    req.observations[0]
        .comic
        .as_mut()
        .unwrap()
        .simple_tags
        .as_mut()
        .unwrap()[0]
        .push('x');
    rejected(&req);
    req.observations[0].comic.as_mut().unwrap().simple_tags =
        Some(vec![String::new(); MAX_STRINGS]);
    assert_eq!(plan(&req).patches.len(), 1);
    req.observations[0].comic.as_mut().unwrap().penciller = Some(vec![String::new()]);
    rejected(&req);
}

fn golden_requests() -> Vec<(&'static str, Request)> {
    let observe = request(vec![
        snapshot(3, false, false, true),
        snapshot(0, false, false, true),
        snapshot(9, true, true, true),
        snapshot(4, false, true, true),
        snapshot(0, true, true, false),
    ]);
    let mut patch = observe.clone();
    patch.phase = Phase::Patch;
    patch.observations = vec![
        Observation {
            index: 1,
            comic: None,
            spider_pages: Some(6),
        },
        Observation {
            index: 2,
            comic: Some(comic(Some(vec!["a".into(), "a".into()]))),
            spider_pages: None,
        },
        Observation {
            index: 3,
            comic: Some(comic(None)),
            spider_pages: None,
        },
    ];
    let mut noop = patch.clone();
    noop.observations.iter_mut().for_each(|fact| {
        fact.comic = None;
        fact.spider_pages = None;
    });
    vec![("observe", observe), ("patch", patch), ("noop", noop)]
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut result, byte| {
        write!(result, "{byte:02x}").unwrap();
        result
    })
}
#[test]
fn shared_exact_cbor_requests_and_envelopes() {
    for (name, request) in golden_requests() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data");
        let request_hex = std::fs::read_to_string(
            root.join(format!("download_metadata_{name}_request.cbor.hex")),
        )
        .unwrap();
        let response_hex = std::fs::read_to_string(
            root.join(format!("download_metadata_{name}_response.cbor.hex")),
        )
        .unwrap();
        let bytes = serde_cbor::to_vec(&request).unwrap();
        assert_eq!(hex(&bytes), request_hex.trim());
        assert_eq!(serde_cbor::from_slice::<Request>(&bytes).unwrap(), request);
        assert_eq!(
            hex(&serde_cbor::to_vec(&build_envelope(&request)).unwrap()),
            response_hex.trim()
        );
    }
}
