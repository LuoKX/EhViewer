use super::*;

fn request(operation: BbCodeOperation) -> BbCodeRequest {
    BbCodeRequest {
        version: 1,
        operation,
        units: "abcd".encode_utf16().collect(),
        origin_units: None,
        spans: vec![],
    }
}
fn span(start: u32, end: u32, flags: u8) -> TextSpan {
    TextSpan { start, end, flags }
}
fn exported(spans: Vec<TextSpan>) -> String {
    let mut r = request(BbCodeOperation::Export);
    r.spans = spans;
    String::from_utf16(&build_envelope(&r).payload.unwrap().units.unwrap()).unwrap()
}

#[test]
fn export_preserves_all_tags_and_source_order_for_equal_bounds() {
    for (flag, tag) in [
        (1, "b"),
        (2, "i"),
        (4, "u"),
        (8, "s"),
        (3, "b"),
        (5, "b"),
        (9, "b"),
        (6, "i"),
        (10, "i"),
        (12, "u"),
        (15, "b"),
    ] {
        assert_eq!(
            exported(vec![span(1, 3, flag)]),
            format!("a[{tag}]bc[/{tag}]d")
        );
    }
    assert_eq!(
        exported(vec![span(0, 4, 2), span(0, 4, 1)]),
        "[i][b]abcd[/b][/i]"
    );
    assert_eq!(
        exported(vec![span(0, 2, 2), span(0, 4, 1)]),
        "[b][i]ab[/i]cd[/b]"
    );
    assert_eq!(exported(vec![]), "abcd");
}
#[test]
fn export_keeps_legacy_crossing_empty_and_unsupported_stack_behavior() {
    assert_eq!(
        exported(vec![span(0, 2, 1), span(1, 3, 2)]),
        "[b]a[i]bc[/i]d"
    );
    assert_eq!(exported(vec![span(0, 4, 1), span(1, 1, 2)]), "[b]a[i]bcd");
    assert_eq!(exported(vec![span(0, 4, 1), span(1, 1, 0)]), "[b]abcd");
    assert_eq!(exported(vec![span(4, 4, 1)]), "abcd");
}
#[test]
fn export_retains_raw_utf16_and_round_trips_cbor() {
    let mut r = request(BbCodeOperation::Export);
    r.units = vec![0xd800, 0xdc00, 0xd800, 0, 0xdc00];
    r.spans = vec![span(2, 4, 4)];
    assert_eq!(
        build_envelope(&r).payload.unwrap().units.unwrap(),
        vec![
            0xd800, 0xdc00, 91, 117, 93, 0xd800, 0, 91, 47, 117, 93, 0xdc00
        ]
    );
    let bytes = serde_cbor::to_vec(&r).unwrap();
    assert_eq!(serde_cbor::from_slice::<BbCodeRequest>(&bytes).unwrap(), r);
}
#[test]
fn edit_actions_cover_equality_insert_delete_replace_and_multi_hunk() {
    for (origin, target, action, edit) in [
        ("abc", "abc", BbCodeAction::Original, None),
        (
            "abc",
            "aXbc",
            BbCodeAction::Splice,
            Some(TextEdit {
                source_start: 1,
                source_end: 1,
                target_start: 1,
                target_end: 2,
            }),
        ),
        (
            "abc",
            "ac",
            BbCodeAction::Splice,
            Some(TextEdit {
                source_start: 1,
                source_end: 2,
                target_start: 1,
                target_end: 1,
            }),
        ),
        ("abc", "aXc", BbCodeAction::Target, None),
        ("aaa", "aaXaa", BbCodeAction::Target, None),
    ] {
        let mut r = request(BbCodeOperation::Edit);
        r.units = target.encode_utf16().collect();
        r.origin_units = Some(origin.encode_utf16().collect());
        let p = build_envelope(&r).payload.unwrap();
        assert_eq!(p.action, action);
        assert_eq!(p.edit, edit);
        assert!(p.units.is_none());
    }
}
#[test]
fn invalid_input_has_fixed_input_free_errors() {
    let base = request(BbCodeOperation::Export);
    let mut invalid = vec![];
    let mut r = base.clone();
    r.version = 2;
    invalid.push(r);
    let mut r = base.clone();
    r.units = vec![0; MAX_TEXT_UNITS + 1];
    invalid.push(r);
    let mut r = base.clone();
    r.origin_units = Some(vec![0; MAX_TEXT_UNITS + 1]);
    invalid.push(r);
    let mut r = base.clone();
    r.spans = vec![span(0, 1, 1); MAX_SPANS + 1];
    invalid.push(r);
    for s in [span(3, 2, 1), span(0, 5, 1), span(0, 1, 16)] {
        let mut r = base.clone();
        r.spans = vec![s];
        invalid.push(r);
    }
    for r in invalid {
        let e = build_envelope(&r);
        assert!(e.payload.is_none());
        assert_eq!(
            e.error,
            Some(RustError::Error("Invalid BBCode input".into()))
        );
    }
    let mut r = base;
    r.origin_units = Some(vec![]);
    assert!(build_envelope(&r).error.is_some());
    r.operation = BbCodeOperation::Edit;
    r.origin_units = None;
    assert!(build_envelope(&r).error.is_some());
    r.origin_units = Some(vec![]);
    r.spans = vec![span(0, 1, 1)];
    assert!(build_envelope(&r).error.is_some());
}
#[test]
fn output_budget_covers_open_text_close_and_complete_result_failure() {
    let units = [1, 2];
    let spans = [span(0, 2, 1)];
    for limit in [0, 2, 3, 4, 8] {
        assert_eq!(export(&units, &spans, limit), Err(()));
    }
    assert_eq!(export(&units, &spans, 9).unwrap().len(), 9);
    let mut r = request(BbCodeOperation::Export);
    r.spans = vec![span(0, 4, 1); MAX_SPANS];
    // 100k supported spans produce 700k tag units, within the declared bound.
    assert_eq!(
        build_envelope(&r).payload.unwrap().units.unwrap().len(),
        700_004
    );
    let mut output = vec![1];
    assert_eq!(append(&mut output, &[2], 1), Err(()));
    assert_eq!(output, vec![1]);
}
