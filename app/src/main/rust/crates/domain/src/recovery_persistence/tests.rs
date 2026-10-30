use super::*;

fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation,
    }
}
fn run(operation: Operation) -> Plan {
    plan(&request(operation)).unwrap()
}
fn text(value: &str) -> Text {
    Text { value: raw(value) }
}
fn read(value: &ByteBuf) -> String {
    String::from_utf16(&units(value).unwrap()).unwrap()
}

#[test]
fn formal_field_selection_preserves_float_bits_and_missing_semantics() {
    for exists in [false, true] {
        for blank in [false, true] {
            for category in [0, -1, i32::MAX] {
                for rating in [0.0_f32, -0.0, f32::NAN, f32::INFINITY, -1.5] {
                    for pages in [-1, 0, 1] {
                        let p = run(Operation::Formal {
                            exists,
                            blank: vec![blank; 6],
                            category,
                            rating_bits: rating.to_bits(),
                            has_tags: !blank,
                            pages,
                        });
                        let mut expected = vec![
                            true,
                            !blank,
                            !blank,
                            !blank,
                            category != 0,
                            !blank,
                            !blank,
                            rating != 0.0,
                            !blank,
                            pages > 0,
                            !blank,
                        ];
                        for value in &mut expected {
                            *value &= exists;
                        }
                        assert_eq!(p.flags, expected);
                    }
                }
            }
        }
    }
}

#[test]
fn artists_trim_without_unicode_conversion_and_keep_first_occurrence() {
    let names = vec![
        text(" A "),
        text("A"),
        text("　B　"),
        text(""),
        text(" \t"),
        Text {
            value: vec![0, 216].into(),
        },
        Text {
            value: vec![0, 216].into(),
        },
    ];
    assert_eq!(
        run(Operation::Artists {
            names,
            whitespace: vec![32, 9, 0x3000]
        })
        .spans,
        [[0, 1, 2], [2, 1, 2], [5, 0, 1]]
    );
    assert!(
        run(Operation::Artists {
            names: vec![],
            whitespace: vec![]
        })
        .spans
        .is_empty()
    );
}

#[test]
fn staging_is_null_only_and_status_priority_is_stable() {
    use RecoveryStatus::{
        Conflict, Incomplete, LimitExceeded, MissingIdentity, Ready, SourceMissing, Unreadable,
    };
    let cases = [
        (Ready, vec![], "READY"),
        (MissingIdentity, vec![], "MISSING_IDENTITY"),
        (Incomplete, vec![], "CONTENT_INCOMPLETE"),
        (Unreadable, vec![], "UNREADABLE"),
        (LimitExceeded, vec![], "LIMIT_EXCEEDED"),
        (SourceMissing, vec![], "SOURCE_MISSING"),
        (Conflict, vec![], "IDENTITY_CONFLICT"),
        (Conflict, vec!["EXISTING_MAPPING_CONFLICT"], "PATH_CONFLICT"),
        (
            Conflict,
            vec![
                "EXISTING_MAPPING_CONFLICT",
                "DUPLICATE_SOURCE",
                "DUPLICATE_SOURCE",
            ],
            "DUPLICATE_SOURCE",
        ),
    ];
    for (status, issues, expected) in cases {
        for action in [
            RecoveryPersistenceAction::Stage,
            RecoveryPersistenceAction::KeepExisting,
            RecoveryPersistenceAction::PromoteFinished,
        ] {
            for available in [false, true] {
                let present = vec![true, false, true, false, true, false];
                let p = run(Operation::Stage {
                    action,
                    status,
                    issues: issues.iter().map(ToString::to_string).collect(),
                    path_available: available,
                    present: present.clone(),
                });
                if action == RecoveryPersistenceAction::KeepExisting {
                    assert_eq!(p.flags, [true]);
                    assert!(p.status.is_none() && p.text.is_none());
                } else {
                    let blocked =
                        action == RecoveryPersistenceAction::PromoteFinished && !available;
                    assert_eq!(&p.flags[1..], present);
                    assert_eq!(
                        p.status.as_deref(),
                        Some(if blocked { "PATH_CONFLICT" } else { expected })
                    );
                    let mut distinct = Vec::new();
                    for issue in &issues {
                        if !distinct.contains(issue) {
                            distinct.push(*issue);
                        }
                    }
                    if blocked {
                        distinct.push("ANDROID_PATH_CONFLICT");
                    }
                    assert_eq!(read(&p.text.unwrap()), distinct.join(","));
                }
            }
        }
    }
}

#[test]
fn failures_do_not_trim_surviving_fragments_and_finalization_observes_all_limits() {
    for input in [
        "",
        " ,,\t",
        "A, A ,A,PROMOTION_CONFLICT",
        "　,A,　A　",
        "PROMOTION_CONFLICT,PROMOTION_CONFLICT",
    ] {
        let p = run(Operation::Failure {
            issues_text: raw(input),
            whitespace: vec![32, 9, 0x3000],
        });
        let mut expected = Vec::new();
        for part in input
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .chain(std::iter::once("PROMOTION_CONFLICT"))
        {
            if !expected.contains(&part) {
                expected.push(part);
            }
        }
        assert_eq!(read(&p.text.unwrap()), expected.join(","));
        assert_eq!(p.status.as_deref(), Some("PATH_CONFLICT"));
    }
    for bits in 0..8 {
        let limits = (0..3).map(|b| bits & (1 << b) != 0).collect();
        assert_eq!(
            run(Operation::Finish { limits }).flags,
            [bits != 0, bits == 0]
        );
    }
    assert_eq!(
        run(Operation::Pending {
            same_root: vec![true, true, false, false],
            missing: vec![true, false, false, true]
        })
        .count,
        Some(1)
    );
    assert_eq!(
        run(Operation::Pending {
            same_root: vec![],
            missing: vec![]
        })
        .count,
        Some(0)
    );
}

#[test]
fn rejects_invalid_shapes_versions_and_budgets() {
    let cases = [
        Operation::Formal {
            exists: true,
            blank: vec![],
            category: 0,
            rating_bits: 0,
            has_tags: false,
            pages: 0,
        },
        Operation::Artists {
            names: vec![text(""); MAX_ITEMS + 1],
            whitespace: vec![],
        },
        Operation::Artists {
            names: vec![
                Text {
                    value: vec![0; 1024 * 1024].into()
                };
                9
            ],
            whitespace: vec![],
        },
        Operation::Artists {
            names: vec![Text {
                value: vec![0].into(),
            }],
            whitespace: vec![],
        },
        Operation::Artists {
            names: vec![Text {
                value: vec![0; 1024 * 1024 + 2].into(),
            }],
            whitespace: vec![],
        },
        Operation::Artists {
            names: vec![],
            whitespace: vec![32, 32],
        },
        Operation::Artists {
            names: vec![],
            whitespace: vec![32; 65_537],
        },
        Operation::Finish { limits: vec![] },
        Operation::Pending {
            same_root: vec![true],
            missing: vec![],
        },
        Operation::Pending {
            same_root: vec![true; MAX_ITEMS + 1],
            missing: vec![false; MAX_ITEMS + 1],
        },
    ];
    for operation in cases {
        assert!(build_envelope(&request(operation)).error.is_some());
    }
    for issues in [
        vec!["x".into()],
        vec![String::new()],
        vec!["A".repeat(65)],
        vec!["A".into(); 65],
    ] {
        assert!(
            plan(&request(Operation::Stage {
                action: RecoveryPersistenceAction::Stage,
                status: RecoveryStatus::Ready,
                issues,
                path_available: true,
                present: vec![true; 6]
            }))
            .is_err()
        );
    }
    assert!(
        plan(&request(Operation::Stage {
            action: RecoveryPersistenceAction::Stage,
            status: RecoveryStatus::Ready,
            issues: vec![],
            path_available: true,
            present: vec![]
        }))
        .is_err()
    );
    assert!(
        build_envelope(&Request {
            version: 2,
            operation: Operation::Finish {
                limits: vec![false; 3]
            }
        })
        .error
        .is_some()
    );
    assert!(
        build_envelope(&request(Operation::Finish {
            limits: vec![false; 3]
        }))
        .payload
        .is_some()
    );
}

#[test]
fn shared_cbor_goldens() {
    let decode = |s: &str| {
        s.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    for (input, output) in [
        (
            include_str!("../../test_data/recovery_artists_request.cbor.hex"),
            include_str!("../../test_data/recovery_artists_response.cbor.hex"),
        ),
        (
            include_str!("../../test_data/recovery_stage_request.cbor.hex"),
            include_str!("../../test_data/recovery_stage_response.cbor.hex"),
        ),
    ] {
        let r: Request = serde_cbor::from_slice(&decode(input)).unwrap();
        assert_eq!(
            serde_cbor::to_vec(&build_envelope(&r)).unwrap(),
            decode(output)
        );
    }
    let invalid =
        r#"{"version":1,"kind":"finish","limits":[false,false,false],"credential":"unused"}"#;
    assert!(serde_json::from_str::<Request>(invalid).is_err());
}
