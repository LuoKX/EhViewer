use super::*;

#[test]
fn cross_store_matrix_and_distinct_sentinels() {
    for source in [i32::MIN, -3, -2, -1, 0, 5, 9, 10, i32::MAX] {
        assert_eq!(
            plan(0, source, 0).unwrap(),
            [if source == -2 { 1 } else { 2 }, 1]
        );
        for destination in std::iter::once(-2).chain(0..=9) {
            let action = if source == destination {
                0
            } else if source == -2 {
                3
            } else if destination == -2 {
                4
            } else {
                5
            };
            assert_eq!(
                plan(1, source, destination).unwrap(),
                [action, i32::from(action != 0)]
            );
        }
        assert_eq!(plan(3, source, 0).unwrap(), [i32::from(source == -2), -1]);
    }
}

#[test]
fn menu_bounds_and_operation_shapes() {
    for count in 1..=11 {
        for index in 0..count {
            assert_eq!(
                plan(2, index, count).unwrap(),
                [if index == 0 { -2 } else { index - 1 }]
            );
        }
    }
    for (op, left, right) in [
        (99, 0, 0),
        (0, 0, 1),
        (1, 0, -1),
        (1, 0, 10),
        (2, -1, 11),
        (2, 11, 11),
        (2, 0, 0),
        (2, 0, 12),
        (3, 0, 1),
    ] {
        assert!(plan(op, left, right).is_err());
    }
}
