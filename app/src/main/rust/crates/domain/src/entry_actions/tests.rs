use super::*;

#[test]
fn partitions_every_boolean_pattern_stably_including_repeated_selections() {
    for bits in 0_u32..256 {
        let facts: Vec<_> = (0..8).map(|i| bits & (1 << i) != 0).collect();
        let result = plan(0, 0, 0, &facts).unwrap();
        let expected: Vec<_> = (0..8)
            .filter(|&i| facts[i])
            .chain((0..8).filter(|&i| !facts[i]))
            .map(|v| i32::try_from(v).unwrap())
            .collect();
        assert_eq!(result[0], i32::try_from(bits.count_ones()).unwrap());
        assert_eq!(&result[1..], expected);
    }
    assert_eq!(plan(0, 0, 0, &[]).unwrap(), [0]);
    assert_eq!(
        plan(0, 0, 0, &vec![true; MAX_FACTS]).unwrap().len(),
        MAX_FACTS + 1
    );
    assert!(plan(0, 0, 0, &vec![false; MAX_FACTS + 1]).is_err());
}

#[test]
fn default_choice_matches_the_original_sequential_fallbacks() {
    for bits in 0_u32..32 {
        let facts: Vec<_> = (0..5).map(|i| bits & (1 << i) != 0).collect();
        let mut direct = facts[0];
        let mut used = false;
        if !direct && facts[1] {
            direct = facts[2] || facts[3];
            used = direct;
        }
        if !direct && facts[4] {
            direct = true;
            used = false;
        }
        assert_eq!(
            plan(1, 0, 0, &facts).unwrap(),
            [i32::from(direct), i32::from(used)]
        );
    }
}

#[test]
fn favorites_preserve_raw_defaults_and_capture_menu_basis() {
    for signed in [false, true] {
        for missing in [false, true] {
            for current in [-9, -2, -1, 0, 9, 10] {
                for default in [i32::MIN, -2, -1, 0, 9, i32::MAX] {
                    let result = plan(2, current, default, &[signed, missing]).unwrap();
                    if !signed {
                        assert_eq!(result, [0, 0, 0, -1]);
                    } else if default == -2 {
                        assert_eq!(
                            result[..3],
                            [
                                1,
                                i32::from(current != -2),
                                i32::from(current >= 0 && missing)
                            ]
                        );
                    } else {
                        assert_eq!(result, [0, 0, 0, if current == -2 { default } else { -2 }]);
                    }
                }
            }
        }
    }
    for (remove, count) in [(false, 11), (true, 12)] {
        for slot in 0..count {
            assert_eq!(
                plan(3, slot, 0, &[remove]).unwrap(),
                [slot - if remove { 2 } else { 1 }]
            );
        }
        assert!(plan(3, -1, 0, &[remove]).is_err());
        assert!(plan(3, count, 0, &[remove]).is_err());
    }
    for slot in 0..=3 {
        assert_eq!(plan(4, slot, 3, &[]).unwrap(), [slot - 1]);
    }
    assert_eq!(plan(4, 0, 0, &[]).unwrap(), [-1]);
    for (op, left, right, facts) in [
        (9, 0, 0, vec![]),
        (1, 0, 0, vec![]),
        (2, 0, 0, vec![true]),
        (3, 0, 0, vec![]),
        (4, 0, -1, vec![]),
        (4, 4, 3, vec![]),
    ] {
        assert!(plan(op, left, right, &facts).is_err());
    }
}
