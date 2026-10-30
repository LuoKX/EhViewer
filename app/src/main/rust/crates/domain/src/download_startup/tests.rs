use super::*;

#[test]
fn preserves_raw_states_except_volatile_workers() {
    let mut values: Vec<i32> = (-1000..1000).collect();
    values.extend([i32::MIN, i32::MAX]);
    let expected: Vec<_> = values
        .iter()
        .map(|&v| if v == 1 || v == 2 { 0 } else { v })
        .collect();
    let projected = normalize_states(&values).unwrap();
    assert_eq!(projected, expected);
    assert_eq!(normalize_states(&projected).unwrap(), projected);
    assert_eq!(values[1001], 1);
    assert_eq!(values[1002], 2);
}

#[test]
fn bounded_empty_and_maximum_snapshots() {
    assert!(normalize_states(&[]).unwrap().is_empty());
    assert_eq!(
        normalize_states(&vec![2; MAX_STATES]).unwrap(),
        vec![0; MAX_STATES]
    );
    assert!(normalize_states(&vec![2; MAX_STATES + 1]).is_err());
}
