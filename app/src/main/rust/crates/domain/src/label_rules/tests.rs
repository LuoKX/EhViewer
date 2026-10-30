use super::*;

#[test]
fn shared_label_edit_golden_preserves_legacy_request_defaults() {
    let decode = |s: &str| {
        s.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let input = decode(include_str!("../../test_data/label_edit_request.cbor.hex"));
    let request: Request = serde_cbor::from_slice(&input).unwrap();
    assert_eq!(serde_cbor::to_vec(&request).unwrap(), input);
    assert_eq!(
        serde_cbor::to_vec(&build_envelope(&request)).unwrap(),
        decode(include_str!("../../test_data/label_edit_response.cbor.hex"))
    );
}

#[test]
fn name_validation_and_settings_menu_have_separate_prefix_semantics() {
    let mut r = request(Action::ValidateName);
    r.blank = Some(false);
    r.reserved = name("default");
    r.name = name("new");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 0);
    r.name = name("a");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 3);
    r.reserved = name("a");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 2);
    r.blank = Some(true);
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r.blank = None;
    assert!(build_envelope(&r).error.is_some());
    r = request(Action::DefaultSelection);
    r.has_default = Some(false);
    assert_eq!(build_envelope(&r).payload.unwrap().value, 0);
    r.has_default = Some(true);
    assert_eq!(build_envelope(&r).payload.unwrap().value, 2);
    r.name = None;
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r.name = name("gone");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r = request(Action::DefaultChoice);
    r.name = None;
    for index in 0..=6 {
        r.index = index;
        assert_eq!(build_envelope(&r).payload.unwrap().value, index - 2);
    }
    r.index = 7;
    assert!(build_envelope(&r).error.is_some());
    r.index = -1;
    assert!(build_envelope(&r).error.is_some());
    r = request(Action::Lookup);
    r.blank = Some(false);
    assert!(build_envelope(&r).error.is_some());
}

#[test]
fn shared_label_wire_golden() {
    let decode = |hex: &str| {
        hex.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let input = decode(include_str!("../../test_data/label_rules_request.cbor.hex"));
    let request: Request = serde_cbor::from_slice(&input).unwrap();
    assert_eq!(serde_cbor::to_vec(&request).unwrap(), input);
    assert_eq!(
        serde_cbor::to_vec(&build_envelope(&request)).unwrap(),
        decode(include_str!(
            "../../test_data/label_rules_response.cbor.hex"
        ))
    );
}
#[allow(
    clippy::unnecessary_wraps,
    reason = "fixture helper for nullable protocol fields"
)]
fn name(s: &str) -> Option<ByteBuf> {
    Some(ByteBuf::from(
        s.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    ))
}
fn request(action: Action) -> Request {
    Request {
        version: 1,
        action,
        names: [Some("a"), Some("A"), Some("a"), None, Some("")]
            .into_iter()
            .map(|s| LabelName {
                value: s.and_then(name),
            })
            .collect(),
        name: name("a"),
        positions: vec![],
        index: 0,
        blank: None,
        reserved: None,
        has_default: None,
    }
}

#[test]
fn exact_first_match_append_uses_count_not_positions_and_null_assignment() {
    let mut r = request(Action::Lookup);
    assert_eq!(build_envelope(&r).payload.unwrap().value, 0);
    r.name = name("A");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r.name = None;
    assert_eq!(build_envelope(&r).payload.unwrap().value, -1);
    r.action = Action::Assign;
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r.name = name("a");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 1);
    r.name = name("new");
    assert_eq!(build_envelope(&r).payload.unwrap().value, 0);
    r.action = Action::Add;
    assert_eq!(build_envelope(&r).payload.unwrap().value, 5);
    r.name = name("");
    assert_eq!(build_envelope(&r).payload.unwrap().value, -1);
    r.name = None;
    assert_eq!(build_envelope(&r).payload.unwrap().value, -1);
    r.action = Action::Matching;
    r.name = name("a");
    assert_eq!(build_envelope(&r).payload.unwrap().indices, [0, 2]);
    r.name = None;
    assert_eq!(build_envelope(&r).payload.unwrap().indices, [3]);
    r.name = Some(ByteBuf::from(vec![0, 0xd8]));
    r.names.push(LabelName {
        value: r.name.clone(),
    });
    assert_eq!(build_envelope(&r).payload.unwrap().indices, [5]);
}

#[test]
fn deletion_changes_source_suffix_even_with_sparse_or_duplicate_positions() {
    let r = Request {
        version: 1,
        action: Action::DeletePositions,
        names: vec![],
        name: None,
        positions: vec![9, 9, -1, i32::MIN],
        index: 1,
        blank: None,
        reserved: None,
        has_default: None,
    };
    assert_eq!(
        build_envelope(&r).payload.unwrap(),
        Plan {
            value: 0,
            indices: vec![2, 3],
            positions: vec![-2, i32::MAX]
        }
    );
    assert!(
        build_envelope(&Request {
            index: 3,
            ..r.clone()
        })
        .payload
        .unwrap()
        .indices
        .is_empty()
    );
    assert!(
        build_envelope(&Request {
            index: -1,
            ..r.clone()
        })
        .error
        .is_some()
    );
    assert!(build_envelope(&Request { index: 4, ..r }).error.is_some());
}

#[test]
fn validates_all_budgets_and_wire_fields_before_planning() {
    let base = request(Action::Lookup);
    for r in [
        Request {
            version: 2,
            ..base.clone()
        },
        Request {
            index: 1,
            ..base.clone()
        },
        Request {
            positions: vec![1],
            ..base.clone()
        },
        Request {
            name: Some(ByteBuf::from(vec![0])),
            ..base.clone()
        },
        Request {
            name: Some(ByteBuf::from(vec![0; 8194])),
            ..base.clone()
        },
        Request {
            names: vec![LabelName { value: None }; 100_001],
            ..base.clone()
        },
        Request {
            names: vec![
                LabelName {
                    value: Some(ByteBuf::from(vec![0; 8192]))
                };
                513
            ],
            ..base.clone()
        },
    ] {
        assert!(build_envelope(&r).error.is_some());
    }
    let r = Request {
        names: vec![LabelName { value: None }; 100_000],
        name: None,
        ..base.clone()
    };
    assert!(build_envelope(&r).payload.is_some());
    let bytes = serde_cbor::to_vec(&base).unwrap();
    assert_eq!(serde_cbor::from_slice::<Request>(&bytes).unwrap(), base);
    assert!(serde_cbor::from_slice::<Request>(&[0xff]).is_err());
}
