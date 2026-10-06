use super::*;
use std::{collections::BTreeMap, fmt::Write as _, path::Path};

fn apply(preview_index: i32, rows: &[(i32, &str)]) -> PreviewTokenRequest {
    PreviewTokenRequest {
        version: 1,
        operation: PreviewTokenOperation::Apply {
            preview_index,
            previews: rows
                .iter()
                .map(|(position, token)| PreviewToken {
                    position: *position,
                    p_token: (*token).into(),
                })
                .collect(),
        },
    }
}
fn lookup(index: i32, preview_per_page: i32) -> PreviewTokenRequest {
    PreviewTokenRequest {
        version: 1,
        operation: PreviewTokenOperation::Lookup {
            index,
            preview_per_page,
        },
    }
}

#[test]
fn first_page_size_and_empty_nonzero_failure_characterize_legacy() {
    assert_eq!(
        plan(&apply(0, &[])).unwrap(),
        PreviewTokenPlan::Apply {
            preview_per_page: 0,
            updates: vec![]
        }
    );
    assert_eq!(
        plan(&apply(0, &[(123, ""), (-4, "x")])).unwrap(),
        PreviewTokenPlan::Apply {
            preview_per_page: 2,
            updates: vec![1]
        }
    );
    for index in [-1, 1, i32::MIN, i32::MAX] {
        assert_eq!(
            plan(&apply(index, &[])),
            Err(RustError::Error("Empty nonzero preview page".into()))
        );
    }
}

#[test]
fn signed_division_truncates_and_wraps_like_jvm_int() {
    for (position, preview_index, expected) in [
        (40, 2, 20),
        (-7, 2, -3),
        (7, -2, -3),
        (-7, -2, 3),
        (i32::MIN, -1, i32::MIN),
        (i32::MAX, 1, i32::MAX),
    ] {
        assert_eq!(
            plan(&apply(preview_index, &[(position, "a")])),
            Ok(PreviewTokenPlan::Apply {
                preview_per_page: expected,
                updates: vec![0]
            })
        );
    }
    for index in [i32::MIN, -7, 0, 7, i32::MAX] {
        for size in [i32::MIN, -1, 0, 1, 2, i32::MAX] {
            assert_eq!(
                plan(&lookup(index, size)).unwrap(),
                PreviewTokenPlan::Lookup {
                    preview_index: if size > 0 { index / size } else { 0 }
                }
            );
        }
    }
}

#[test]
fn sparse_deltas_keep_duplicate_order_empty_intervening_rows_and_current_map() {
    let request = apply(
        2,
        &[(40, "first"), (40, ""), (-1, "negative"), (40, "last")],
    );
    let PreviewTokenOperation::Apply { previews, .. } = &request.operation else {
        panic!()
    };
    let PreviewTokenPlan::Apply {
        updates,
        preview_per_page,
    } = plan(&request).unwrap()
    else {
        panic!()
    };
    assert_eq!(preview_per_page, 20);
    assert_eq!(updates, vec![0, 2, 3]);
    let mut current = BTreeMap::from([(40, "newer".to_owned()), (99, "retained".into())]);
    for index in updates {
        current.insert(previews[index].position, previews[index].p_token.clone());
    }
    assert_eq!(
        current,
        BTreeMap::from([
            (-1, "negative".into()),
            (40, "last".into()),
            (99, "retained".into())
        ])
    );
}

#[test]
fn count_and_token_budgets_are_inclusive_and_errors_fixed() {
    let mut request = apply(0, &[]);
    let PreviewTokenOperation::Apply { previews, .. } = &mut request.operation else {
        panic!()
    };
    *previews = vec![
        PreviewToken {
            position: -1,
            p_token: String::new()
        };
        MAX_PREVIEWS
    ];
    assert!(plan(&request).is_ok());
    let PreviewTokenOperation::Apply { previews, .. } = &mut request.operation else {
        panic!()
    };
    previews.push(PreviewToken {
        position: 0,
        p_token: "secret-token".into(),
    });
    assert_eq!(plan(&request), Err(invalid()));
    for (count, token_size, accepted) in [
        (1, MAX_TOKEN_BYTES, true),
        (1, MAX_TOKEN_BYTES + 1, false),
        (16, MAX_TOKEN_BYTES, true),
        (17, MAX_TOKEN_BYTES, false),
    ] {
        request.operation = PreviewTokenOperation::Apply {
            preview_index: 0,
            previews: vec![
                PreviewToken {
                    position: 0,
                    p_token: "x".repeat(token_size)
                };
                count
            ],
        };
        assert_eq!(plan(&request).is_ok(), accepted);
        if !accepted {
            assert_eq!(plan(&request).unwrap_err(), invalid());
        }
    }
    request.version = 2;
    assert_eq!(plan(&request), Err(invalid()));
    assert_eq!(build_envelope(&request).error, Some(invalid()));
}

#[test]
fn malformed_closed_operations_reject_unknown_fields_and_widths() {
    for json in [
        r#"{"version":1,"kind":"secret-token"}"#,
        r#"{"version":1,"kind":"lookup","index":2147483648,"previewPerPage":1}"#,
        r#"{"version":1,"kind":"lookup","index":0}"#,
        r#"{"version":1,"kind":"apply","previewIndex":1,"previews":[{"position":0}]}"#,
        r#"{"version":1,"kind":"lookup","index":0,"previewPerPage":1,"previews":[]}"#,
    ] {
        assert!(
            serde_json::from_str::<PreviewTokenRequest>(json).is_err(),
            "{json}"
        );
    }
}

fn fixture<T: Serialize + for<'de> Deserialize<'de> + PartialEq>(name: &str, value: &T) {
    let bytes = serde_cbor::to_vec(value).unwrap();
    let hex = bytes.iter().fold(String::new(), |mut out, byte| {
        write!(out, "{byte:02x}").unwrap();
        out
    }) + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test_data")
        .join(format!("preview_tokens_{name}.cbor.hex"));
    if std::env::var_os("UPDATE_PREVIEW_TOKEN_GOLDENS").is_some() {
        std::fs::write(&path, &hex).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), hex);
    assert!(serde_cbor::from_slice::<T>(&bytes).unwrap() == *value);
}

#[test]
fn exact_shared_request_envelope_goldens() {
    for (name, request) in [
        ("lookup", lookup(-7, 2)),
        ("fallback", lookup(i32::MAX, 0)),
        (
            "apply",
            apply(
                2,
                &[(40, "first"), (40, ""), (-1, "negative"), (40, "last")],
            ),
        ),
        ("empty_first", apply(0, &[])),
        ("empty_nonzero", apply(1, &[])),
        ("overflow", apply(-1, &[(i32::MIN, "opaque")])),
        (
            "version",
            PreviewTokenRequest {
                version: 2,
                ..lookup(0, 1)
            },
        ),
    ] {
        fixture(&format!("{name}_request"), &request);
        let envelope = build_envelope(&request);
        fixture(&format!("{name}_response"), &envelope);
        if let Some(plan) = envelope.payload {
            fixture(&format!("{name}_plan"), &plan);
        }
    }
}
