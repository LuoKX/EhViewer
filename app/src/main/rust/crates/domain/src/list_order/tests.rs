use super::*;

#[test]
fn shared_drag_completion_golden() {
    let decode = |s: &str| {
        s.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let input = decode(include_str!(
        "../../test_data/list_order_finish_request.cbor.hex"
    ));
    let request: Request = serde_cbor::from_slice(&input).unwrap();
    assert_eq!(serde_cbor::to_vec(&request).unwrap(), input);
    assert_eq!(
        serde_cbor::to_vec(&build_envelope(&request)).unwrap(),
        decode(include_str!(
            "../../test_data/list_order_finish_response.cbor.hex"
        ))
    );
}
fn request(action: Action, count: i32, from: i32, to: i32) -> Request {
    Request {
        version: 1,
        action,
        count,
        from,
        to,
        headers: 0,
        positions: vec![],
    }
}
fn plan(r: &Request) -> Plan {
    build_envelope(r).payload.unwrap()
}

#[test]
fn every_small_move_and_finish_matches_the_original_list_operations() {
    for count in 1..10 {
        for from in 0..count {
            for to in 0..count {
                for headers in [0, 2] {
                    let r = Request {
                        headers,
                        ..request(Action::Move, count, from + headers, to + headers)
                    };
                    let p = plan(&r);
                    assert_eq!((p.from, p.to), (Some(from), Some(to)));
                    assert!(p.indices.is_empty() && p.positions.is_empty());
                    let p = plan(&request(Action::Finish, count, from, to));
                    let expected: Vec<i32> = if from == to {
                        vec![]
                    } else {
                        (from.min(to)..=from.max(to)).collect()
                    };
                    assert_eq!(p.indices, expected);
                    assert_eq!(p.positions, expected);
                }
            }
        }
    }
    assert!(
        plan(&request(Action::Finish, 0, -1, i32::MIN))
            .indices
            .is_empty()
    );
    assert!(plan(&request(Action::Finish, 0, 55, 55)).indices.is_empty());
}

#[test]
fn suffix_preserves_sparse_duplicate_and_wrapped_positions() {
    let positions = vec![7, 7, i32::MIN, i32::MAX, -2];
    for index in 0..5 {
        let p = plan(&Request {
            positions: positions.clone(),
            ..request(Action::Delete, 5, index, 0)
        });
        let expected: Vec<_> = (index + 1..5).collect();
        assert_eq!(p.indices, expected);
        assert_eq!(
            p.positions,
            positions
                .iter()
                .skip(usize::try_from(index + 1).unwrap())
                .map(|p| p.wrapping_sub(1))
                .collect::<Vec<_>>()
        );
    }
    assert!(delete_positions(&[], 0).is_err());
    assert!(delete_positions(&[1], -1).is_err());
    assert!(delete_positions(&vec![0; MAX_ITEMS + 1], 0).is_err());
}

#[test]
fn rejects_unknown_shapes_bounds_and_overflow_without_partial_plans() {
    let base = request(Action::Move, 3, 0, 2);
    for r in [
        Request {
            version: 2,
            ..base.clone()
        },
        Request {
            count: -1,
            ..base.clone()
        },
        Request {
            count: 100_001,
            ..base.clone()
        },
        Request {
            positions: vec![0],
            ..base.clone()
        },
        Request {
            headers: -1,
            ..base.clone()
        },
        Request {
            from: -1,
            ..base.clone()
        },
        Request {
            to: 3,
            ..base.clone()
        },
        Request {
            from: i32::MIN,
            headers: 2,
            ..base.clone()
        },
        Request {
            to: i32::MIN,
            headers: 2,
            from: 2,
            ..base.clone()
        },
        Request {
            action: Action::Finish,
            headers: 2,
            ..base.clone()
        },
        Request {
            action: Action::Delete,
            ..base.clone()
        },
        Request {
            action: Action::Delete,
            to: 0,
            ..base
        },
    ] {
        assert!(build_envelope(&r).error.is_some());
    }
    assert!(
        build_envelope(&request(Action::Move, 0, 0, 0))
            .error
            .is_some()
    );
    let r = request(Action::Finish, 100_000, 0, 99_999);
    assert_eq!(plan(&r).indices.len(), MAX_ITEMS);
    let bytes = serde_cbor::to_vec(&r).unwrap();
    assert_eq!(serde_cbor::from_slice::<Request>(&bytes).unwrap(), r);
}
