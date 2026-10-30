use super::*;

#[test]
fn shared_cbor_goldens_cover_both_dialog_phases() {
    let decode = |s: &str| {
        s.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    for (input, output) in [
        (
            include_str!("../../test_data/quick_search_check_request.cbor.hex"),
            include_str!("../../test_data/quick_search_check_response.cbor.hex"),
        ),
        (
            include_str!("../../test_data/quick_search_create_request.cbor.hex"),
            include_str!("../../test_data/quick_search_create_response.cbor.hex"),
        ),
    ] {
        let bytes = decode(input);
        let request: Request = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(serde_cbor::to_vec(&request).unwrap(), bytes);
        assert_eq!(
            serde_cbor::to_vec(&build_envelope(&request)).unwrap(),
            decode(output)
        );
    }
}
fn raw(s: &str) -> ByteBuf {
    ByteBuf::from(
        s.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}
fn query() -> QuickSearchSnapshot {
    QuickSearchSnapshot {
        mode: 0,
        category: 1,
        keyword: None,
        advanced_search: -1,
        minimum_rating: 0,
        page_from: 1,
        page_to: 2,
    }
}
fn request(kind: Kind) -> Request {
    Request {
        version: 1,
        kind,
        current: None,
        saved: vec![],
        first_gid: None,
        digits: vec![],
        input: None,
        whitespace: vec![],
        save_progress: None,
        next: None,
        names: vec![],
    }
}
fn checked(names: &[&str], gid: i64) -> Request {
    Request {
        current: Some(query()),
        first_gid: Some(gid),
        saved: names
            .iter()
            .map(|s| Saved {
                query: query(),
                name: raw(s),
            })
            .collect(),
        ..request(Kind::Check)
    }
}
fn create(input: &str) -> Request {
    Request {
        input: Some(raw(input)),
        whitespace: vec![32, 9, 10, 0x2003],
        save_progress: Some(true),
        next: Some(42),
        ..request(Kind::Create)
    }
}
fn plan(r: &Request) -> Plan {
    build_envelope(r).payload.unwrap()
}

#[test]
fn eligibility_and_first_duplicate_reuse_query_equality() {
    assert!(!creation_allowed(4));
    assert!(creation_allowed(-1));
    let mut r = checked(&["missing", "a@+42", "b@42", "x@1@42"], 41);
    assert_eq!(plan(&r).duplicate, Some(1));
    assert_eq!(plan(&r).next, Some(42));
    r.saved[1].query.category = 2;
    assert_eq!(plan(&r).duplicate, Some(2));
    r.saved[2].query.keyword = Some(String::new());
    assert_eq!(plan(&r).duplicate, Some(3));
    r.saved[3].name = raw("x@42@");
    assert_eq!(plan(&r).duplicate, None);
    r.current.as_mut().unwrap().mode = 4;
    assert!(build_envelope(&r).error.is_some());
}

#[test]
fn signed_progress_limits_last_suffix_and_platform_digits() {
    for (suffix, expected) in [
        ("0", Some(0)),
        ("+0", Some(0)),
        ("-0", Some(0)),
        ("00042", Some(42)),
        ("-42", Some(-42)),
        ("9223372036854775807", Some(i64::MAX)),
        ("-9223372036854775808", Some(i64::MIN)),
        ("9223372036854775808", None),
        ("-9223372036854775809", None),
        ("", None),
        ("+", None),
        ("-", None),
        (" 42", None),
        ("42 ", None),
        ("1.0", None),
        ("1e2", None),
        ("x", None),
        ("42@41", Some(41)),
        ("999999999999999999999999999999", None),
    ] {
        assert_eq!(
            parse_progress(&raw(&format!("name@{suffix}")), &HashMap::new()),
            expected,
            "{suffix}"
        );
    }
    assert_eq!(parse_progress(&raw("42"), &HashMap::new()), None);
    let mut r = checked(&["name@٤٢"], 41);
    r.digits = vec![
        Digit {
            unit: 0x664,
            value: 4,
        },
        Digit {
            unit: 0x662,
            value: 2,
        },
    ];
    assert_eq!(plan(&r).duplicate, Some(0));
    assert_eq!(
        plan(&checked(&["x@-9223372036854775808"], i64::MAX)).duplicate,
        Some(0)
    );
    let mut lone = checked(&[], 41);
    lone.saved.push(Saved {
        query: query(),
        name: ByteBuf::from(vec![0, 0xd8, b'@', 0, b'4', 0, b'2', 0]),
    });
    assert_eq!(plan(&lone).duplicate, Some(0));
}

#[test]
fn creation_trims_only_observed_units_and_checks_final_raw_name() {
    let mut r = create("\u{2003} \tbook\n");
    assert_eq!(plan(&r).name, Some(raw("book@42")));
    assert_eq!(plan(&r).position, Some(0));
    r.names.push(Name {
        value: raw("book@42"),
    });
    assert_eq!(plan(&r).error, Some(NameError::Duplicate));
    r.save_progress = Some(false);
    assert_eq!(plan(&r).name, Some(raw("book")));
    assert_eq!(plan(&r).position, Some(1));
    r.input = Some(raw("\t\n\u{2003} "));
    assert_eq!(plan(&r).error, Some(NameError::Empty));
    r.input = Some(ByteBuf::from(vec![32, 0, 0, 0xd8, 32, 0]));
    assert_eq!(plan(&r).name.unwrap().as_ref(), &[0, 0xd8]);
    r = create("X@1");
    r.next = Some(i64::MIN);
    assert_eq!(plan(&r).name, Some(raw("X@1@-9223372036854775808")));
    r = create("\u{85}");
    assert_eq!(plan(&r).name, Some(raw("\u{85}@42")));
}

#[test]
fn malformed_shapes_and_all_input_budgets_reject_before_planning() {
    let check = checked(&[], 0);
    let mut invalids = vec![
        request(Kind::Check),
        request(Kind::Create),
        Request {
            version: 2,
            ..check.clone()
        },
        Request {
            input: Some(raw("private")),
            ..check.clone()
        },
        Request {
            digits: vec![Digit {
                unit: 128,
                value: 10,
            }],
            ..check.clone()
        },
        Request {
            digits: vec![Digit { unit: 48, value: 0 }],
            ..check.clone()
        },
        Request {
            digits: vec![
                Digit {
                    unit: 128,
                    value: 1
                };
                2
            ],
            ..check.clone()
        },
        Request {
            digits: vec![
                Digit {
                    unit: 128,
                    value: 1
                };
                65_537
            ],
            ..check.clone()
        },
        Request {
            saved: vec![
                Saved {
                    query: query(),
                    name: raw("")
                };
                MAX_ITEMS + 1
            ],
            ..check.clone()
        },
    ];
    for size in [1, MAX_NAME_BYTES + 2] {
        invalids.push(Request {
            input: Some(ByteBuf::from(vec![0; size])),
            ..create("x")
        });
    }
    invalids.push(Request {
        names: vec![Name { value: raw("") }; MAX_ITEMS + 1],
        ..create("x")
    });
    invalids.push(Request {
        names: vec![
            Name {
                value: ByteBuf::from(vec![0; MAX_NAME_BYTES])
            };
            257
        ],
        ..create("x")
    });
    invalids.push(Request {
        whitespace: vec![32; 65_537],
        ..create("x")
    });
    invalids.push(Request {
        whitespace: vec![32, 32],
        ..create("x")
    });
    invalids.push(Request {
        current: Some(query()),
        ..create("x")
    });
    let mut large = check;
    large.current.as_mut().unwrap().keyword = Some("x".repeat(4 * 1024 * 1024 + 1));
    invalids.push(large);
    for r in invalids {
        assert!(build_envelope(&r).error.is_some());
    }
    let mut boundary = create(&"x".repeat(MAX_NAME_BYTES / 2));
    assert!(build_envelope(&boundary).error.is_some());
    boundary.save_progress = Some(false);
    assert!(build_envelope(&boundary).payload.is_some());
    let bytes = serde_cbor::to_vec(&boundary).unwrap();
    assert_eq!(serde_cbor::from_slice::<Request>(&bytes).unwrap(), boundary);
}
