use super::*;
fn span(start: u32, end: u32, flags: u8) -> TextSpan {
    TextSpan { start, end, flags }
}
fn request(spans: Vec<TextSpan>) -> TextSpansRequest {
    TextSpansRequest {
        version: 1,
        text_length: 100,
        spans,
    }
}
#[test]
fn merges_overlap_adjacency_duplicates_and_containment_by_style() {
    let plan = build_envelope(&request(vec![
        span(7, 9, 1),
        span(1, 4, 3),
        span(4, 7, 1),
        span(2, 3, 1),
        span(1, 4, 3),
        span(10, 11, 4),
        span(12, 13, 8),
    ]));
    assert_eq!(
        plan.payload.unwrap(),
        vec![
            span(1, 9, 1),
            span(1, 4, 2),
            span(10, 11, 4),
            span(12, 13, 8)
        ]
    );
}
#[test]
fn ignores_empty_and_unrecognized_spans_preserving_separate_runs() {
    assert_eq!(
        build_envelope(&request(vec![
            span(3, 3, 15),
            span(0, 10, 0),
            span(8, 9, 2),
            span(1, 2, 2)
        ]))
        .payload
        .unwrap(),
        vec![span(1, 2, 2), span(8, 9, 2)]
    );
    assert!(build_envelope(&request(vec![])).payload.unwrap().is_empty());
}
#[test]
fn rejects_version_bounds_reversed_ranges_and_unknown_flags() {
    for s in [span(9, 8, 1), span(0, 101, 1), span(0, 1, 16)] {
        assert!(build_envelope(&request(vec![s])).error.is_some());
    }
    let mut input = request(vec![]);
    input.version = 2;
    assert!(build_envelope(&input).error.is_some());
    input.version = 1;
    input.text_length = u32::MAX;
    assert!(build_envelope(&input).error.is_some());
    input.text_length = 100;
    input.spans = vec![span(0, 1, 1); MAX_SPANS + 1];
    assert!(build_envelope(&input).error.is_some());
}
#[test]
fn preserves_int_max_endpoint_without_inclusive_range_overflow() {
    let request = TextSpansRequest {
        version: 1,
        text_length: MAX_TEXT_LENGTH,
        spans: vec![span(MAX_TEXT_LENGTH - 1, MAX_TEXT_LENGTH, 15)],
    };
    assert_eq!(build_envelope(&request).payload.unwrap().len(), 4);
}
