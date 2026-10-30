use super::*;

#[test]
fn all_phase_combinations_preserve_write_and_cleanup_conditions() {
    for a in 0..=1 {
        for b in 0..=1 {
            assert_eq!(plan(0, &[a, b]).unwrap(), [i32::from(a == 1 && b == 0)]);
            assert_eq!(plan(1, &[a]).unwrap(), [1 - a]);
            assert_eq!(plan(3, &[a]).unwrap(), [a, 0]);
            assert_eq!(
                plan(4, &[a, b]).unwrap(),
                [if a == 1 { 2 } else { 0 }, i32::from(a == 1 && b == 1)]
            );
            assert_eq!(
                plan(5, &[a, b]).unwrap(),
                [if a == 1 && b == 1 { 3 } else { 2 }, 0]
            );
            assert_eq!(plan(6, &[a]).unwrap(), [2, a]);
        }
    }
}

#[test]
fn last_suffix_preserves_case_spaces_and_java_units() {
    for (key, start) in [
        ("", -1),
        ("nodot", -1),
        ("a.", -1),
        ("a.\t　", -1),
        ("a.jpg", 2),
        ("a.b.PNG", 4),
        ("a. x ", 2),
    ] {
        let mut facts = vec![i32::try_from(key.encode_utf16().count()).unwrap()];
        facts.extend(key.encode_utf16().map(i32::from));
        facts.extend([9, 32, 0x3000]);
        assert_eq!(plan(2, &facts).unwrap(), [start]);
    }
    assert_eq!(plan(2, &[3, 46, 0xD800, 0xDC00]).unwrap(), [1]);
}

#[test]
fn malformed_frames_fail_closed() {
    for (op, facts) in [
        (9, vec![]),
        (0, vec![2, 0]),
        (2, vec![-1]),
        (2, vec![3, 46]),
        (2, vec![1, 65_536]),
        (2, vec![1, 46, -1]),
    ] {
        assert!(plan(op, &facts).is_err());
    }
    assert!(plan(2, &vec![0; MAX_FACTS + 1]).is_err());
}
