use super::*;

#[test]
fn budgets_and_image_accounting_preserve_oom_fix() {
    for (heap, ram, expected) in [
        (256 * 1024 * 1024, 8 * 1024 * 1024 * 1024, 64 * 1024 * 1024),
        (128 * 1024 * 1024, 4 * 1024 * 1024 * 1024, 32 * 1024 * 1024),
        (256 * 1024 * 1024, 128 * 1024 * 1024, 16 * 1024 * 1024),
        (i64::MAX, i64::MAX, 128 * 1024 * 1024),
        (256 * 1024 * 1024, 0, 64 * 1024 * 1024),
        (256 * 1024 * 1024, -1, 64 * 1024 * 1024),
        (0, 1, 1),
        (-1, -1, 1),
    ] {
        let request = ReaderPolicyRequest::Memory {
            app_max_memory: heap,
            total_memory: ram,
        };
        assert_eq!(
            plan(&request).unwrap(),
            ReaderPolicyPlan::Memory {
                cache_size: expected,
                decode_concurrency: 2,
                decode_attempts: 2
            }
        );
        assert!(build_envelope(1, &request).is_ok());
        assert!(!build_envelope(2, &request).is_ok());
    }
    assert_eq!(image_entry_size(i64::MAX, 64).unwrap(), 64);
    assert_eq!(image_entry_size(i64::MIN, 64).unwrap(), 1);
    assert_eq!(image_entry_size(42, 64).unwrap(), 42);
    assert!(image_entry_size(0, 0).is_err());
}

#[test]
fn all_page_transitions_and_repeated_notifications() {
    let states = [
        PageState::None,
        PageState::Downloading,
        PageState::Finished,
        PageState::Failed,
    ];
    for old in states {
        for new in states {
            let done = |state| matches!(state, PageState::Finished | PageState::Failed);
            let request = ReaderPolicyRequest::PageTransition {
                pages: 2,
                finished: 1 + i32::from(old == PageState::Finished),
                downloaded: 1 + i32::from(done(old)),
                old_state: old,
                new_state: new,
            };
            let ReaderPolicyPlan::PageTransition {
                finished,
                downloaded,
                notification,
                completion,
            } = plan(&request).unwrap()
            else {
                panic!("wrong plan")
            };
            assert_eq!(finished, 1 + i32::from(new == PageState::Finished));
            assert_eq!(downloaded, 1 + i32::from(done(new)));
            assert_eq!(
                notification,
                match new {
                    PageState::Finished => PageNotification::Success,
                    PageState::Failed => PageNotification::Failure,
                    _ => PageNotification::None,
                }
            );
            assert_eq!(
                completion,
                match new {
                    PageState::Finished => PageCompletion::Archive,
                    PageState::Failed => PageCompletion::Failure,
                    _ => PageCompletion::None,
                }
            );
        }
    }
    for (pages, finished, downloaded, old_state, new_state) in [
        (0, 0, 0, PageState::None, PageState::None),
        (1, -1, 0, PageState::None, PageState::None),
        (1, 1, 0, PageState::None, PageState::None),
        (1, 0, 2, PageState::None, PageState::None),
        (1, 0, 0, PageState::Finished, PageState::None),
        (1, 1, 1, PageState::None, PageState::Finished),
    ] {
        assert!(
            plan(&ReaderPolicyRequest::PageTransition {
                pages,
                finished,
                downloaded,
                old_state,
                new_state
            })
            .is_err()
        );
    }
}

#[test]
fn ads_exact_quota_and_float_speed_compatibility() {
    for (index, enabled, expected) in [
        (10, true, false),
        (11, true, true),
        (19, true, true),
        (19, false, false),
    ] {
        assert_eq!(
            plan(&ReaderPolicyRequest::Ads {
                index: Some(index),
                pages: Some(20),
                enabled,
                tags: Some(vec!["other".into(), "misc:extraneous ads".into()])
            })
            .unwrap(),
            ReaderPolicyPlan::Ads {
                detect: expected,
                has_ads: true
            }
        );
    }
    assert_eq!(
        plan(&ReaderPolicyRequest::Ads {
            index: None,
            pages: None,
            enabled: true,
            tags: None
        })
        .unwrap(),
        ReaderPolicyPlan::Ads {
            detect: false,
            has_ads: false
        }
    );
    assert_eq!(
        plan(&ReaderPolicyRequest::Ads {
            index: None,
            pages: None,
            enabled: false,
            tags: Some(vec!["EXTRANEOUS ADS".into()])
        })
        .unwrap(),
        ReaderPolicyPlan::Ads {
            detect: false,
            has_ads: false
        }
    );
    assert!(
        plan(&ReaderPolicyRequest::Ads {
            index: Some(1),
            pages: None,
            enabled: true,
            tags: None
        })
        .is_err()
    );
    for url in [
        "https://ehgt.org/g/509.gif",
        "https://ehgt.org/g/509s.gif",
        "https://exhentai.org/img/509.gif",
        "https://exhentai.org/img/509s.gif",
    ] {
        assert_eq!(
            plan(&ReaderPolicyRequest::Quota { url: url.into() }).unwrap(),
            ReaderPolicyPlan::Quota { exceeded: true }
        );
    }
    for url in [
        "http://ehgt.org/g/509.gif",
        "https://ehgt.org/g/509.gif?x",
        "https://ehgt.org/g/509.GIF",
    ] {
        assert_eq!(
            plan(&ReaderPolicyRequest::Quota { url: url.into() }).unwrap(),
            ReaderPolicyPlan::Quota { exceeded: false }
        );
    }
    for (level, expected) in [
        (3, 0),
        (0, 1),
        (-1, 1),
        (-2, 0),
        (4, 16),
        (30, 1_073_741_824),
        (31, i32::MAX),
        (i32::MIN, 0),
        (i32::MAX, i32::MAX),
    ] {
        assert_eq!(
            plan(&ReaderPolicyRequest::SpeedLevel { level }).unwrap(),
            ReaderPolicyPlan::SpeedLevel {
                kib_per_second: expected
            }
        );
    }
}

#[test]
fn wire_round_trips() {
    for request in [
        ReaderPolicyRequest::Memory {
            app_max_memory: 256 * 1024 * 1024,
            total_memory: 0,
        },
        ReaderPolicyRequest::PageTransition {
            pages: 1,
            finished: 0,
            downloaded: 0,
            old_state: PageState::None,
            new_state: PageState::Finished,
        },
        ReaderPolicyRequest::Ads {
            index: None,
            pages: None,
            enabled: false,
            tags: None,
        },
        ReaderPolicyRequest::Quota {
            url: "image".into(),
        },
        ReaderPolicyRequest::SpeedLevel { level: 4 },
    ] {
        let decoded: ReaderPolicyRequest =
            serde_cbor::from_slice(&serde_cbor::to_vec(&request).unwrap()).unwrap();
        let result = build_envelope(1, &decoded);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<ReaderPolicyPlan>>(
                &serde_cbor::to_vec(&result).unwrap()
            )
            .unwrap(),
            result
        );
    }
}
