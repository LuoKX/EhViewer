use super::*;

#[test]
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "matches the existing case-sensitive JVM suffix rule"
)]
fn entry_mode_sorting_and_layout_preserve_platform_choices() {
    for kind in 0..=2 {
        for page in [-1, 0, 8] {
            for archive in 0..=1 {
                let p = plan(0, &[kind, page, archive]).unwrap();
                assert_eq!(
                    p,
                    [
                        i32::from(kind == 0 && page == -1),
                        if kind == 2 {
                            2
                        } else {
                            i32::from(kind == 1 || archive == 1)
                        }
                    ]
                );
            }
        }
    }
    for value in -1..=6 {
        for vertical in 0..=1 {
            assert_eq!(
                plan(10, &[value, vertical]).unwrap(),
                [if (1..=5).contains(&value) {
                    value
                } else if vertical == 1 {
                    1
                } else {
                    4
                }]
            );
        }
    }
    for preference in [-1, 0, 1, 2, 3, 4, 5, 6, i32::MAX] {
        for tag in [
            "",
            "webtoon",
            "other:webtoon",
            "notwebtoon",
            "Webtoon",
            "webtoon ",
        ] {
            let mut facts = vec![preference, i32::try_from(tag.len()).unwrap()];
            facts.extend(tag.encode_utf16().map(i32::from));
            let expected = if (1..=5).contains(&preference) {
                preference
            } else if tag.ends_with("webtoon") {
                4
            } else {
                2
            };
            assert_eq!(
                plan(1, &facts).unwrap(),
                [
                    expected,
                    i32::from(expected >= 3),
                    i32::from(expected >= 4),
                    i32::from(expected == 2)
                ]
            );
        }
    }
    assert_eq!(plan(1, &[0]).unwrap()[0], 2);
    for info in 0..=1 {
        for name in ["a.zip", "a.ZIP", "a.cbz", "x.zip ", ""] {
            let mut facts = vec![info];
            facts.extend(name.encode_utf16().map(i32::from));
            assert_eq!(
                plan(7, &facts).unwrap(),
                [i32::from(info == 0 || name.ends_with(".zip"))]
            );
        }
    }
    for rtl in 0..=1 {
        for system in 0..=1 {
            for vertical in 0..=1 {
                assert_eq!(
                    plan(9, &[rtl, system, vertical]).unwrap(),
                    [
                        i32::from(vertical == 0 && rtl == 1),
                        i32::from(vertical == 0 && rtl != system)
                    ]
                );
            }
        }
    }
}

#[test]
fn region_edges_and_all_inversions_use_ieee_single_precision() {
    for preference in 0..=6 {
        for vertical in [false, true] {
            for x_invert in [false, true] {
                for y_invert in [false, true] {
                    let p = plan(
                        2,
                        &[
                            preference,
                            i32::from(vertical),
                            i32::from(x_invert),
                            i32::from(y_invert),
                        ],
                    )
                    .unwrap();
                    let base = regions(preference, vertical, false, false);
                    assert_eq!(p.len(), base.len());
                    for (result, original) in p.chunks_exact(5).zip(base.chunks_exact(5)) {
                        assert_eq!(result[0], original[0]);
                        let x = if x_invert {
                            [1.0 - float(original[3]), 1.0 - float(original[1])]
                        } else {
                            [float(original[1]), float(original[3])]
                        };
                        let y = if y_invert {
                            [1.0 - float(original[4]), 1.0 - float(original[2])]
                        } else {
                            [float(original[2]), float(original[4])]
                        };
                        assert_eq!(
                            &result[1..],
                            &[bits(x[0]), bits(y[0]), bits(x[1]), bits(y[1])]
                        );
                    }
                    for x in [f32::NAN, f32::INFINITY, -1.0, 0.0, 0.33, 0.66, 1.0] {
                        for y in [-1.0, 0.0, 0.33, 0.66, 1.0, f32::NAN] {
                            let expected = p
                                .chunks_exact(5)
                                .find(|r| {
                                    x >= float(r[1])
                                        && x < float(r[3])
                                        && y >= float(r[2])
                                        && y < float(r[4])
                                })
                                .map_or(0, |r| r[0]);
                            assert_eq!(hit_region(bits(x), bits(y), &p).unwrap(), expected);
                        }
                    }
                }
            }
        }
    }
    let overlap = [
        1,
        bits(0.0),
        bits(0.0),
        bits(1.0),
        bits(1.0),
        2,
        bits(0.0),
        bits(0.0),
        bits(1.0),
        bits(1.0),
    ];
    assert_eq!(hit_region(bits(0.5), bits(0.5), &overlap).unwrap(), 1);
}

#[test]
fn tap_and_pan_plans_keep_rtl_and_webtoon_distinct() {
    for hit in 0..=4 {
        for rtl in [false, true] {
            for webtoon in [false, true] {
                let mut facts = vec![bits(0.5), bits(0.5), i32::from(rtl), i32::from(webtoon)];
                if hit != 0 {
                    facts.extend([hit, bits(0.0), bits(0.0), bits(1.0), bits(1.0)]);
                }
                let expected = if hit == 0 {
                    [0, 0]
                } else if webtoon {
                    [if hit == 2 || hit == 4 { 2 } else { 1 }, 0]
                } else {
                    match hit {
                        1 => [1, if rtl { 1 } else { 2 }],
                        2 => [2, if rtl { 2 } else { 1 }],
                        3 => [if rtl { 2 } else { 1 }, 2],
                        _ => [if rtl { 1 } else { 2 }, 1],
                    }
                };
                assert_eq!(plan(3, &facts).unwrap(), expected);
            }
        }
    }
    for available in 0..=1 {
        for remaining in [
            -1.0,
            1.0,
            f32::from_bits(1.0_f32.to_bits() + 1),
            f32::NAN,
            f32::INFINITY,
        ] {
            assert_eq!(
                plan(4, &[available, bits(remaining)]).unwrap(),
                [i32::from(available == 1 && remaining > 1.0)]
            );
        }
    }
}

#[test]
fn key_page_and_double_tap_boundary_matrices() {
    for key in 0..=4 {
        for down in 0..=1 {
            for volume in 0..=1 {
                for inverted in 0..=1 {
                    let expected = if down == 0 {
                        0
                    } else if key < 3 {
                        key
                    } else if volume == 0 {
                        0
                    } else if (key == 3) == (inverted == 0) {
                        1
                    } else {
                        2
                    };
                    assert_eq!(plan(5, &[key, down, volume, inverted]).unwrap(), [expected]);
                }
            }
        }
    }
    for current in [i32::MIN, -1, 0, 1, 8, i32::MAX] {
        for count in [0, 1, 9, i32::MAX] {
            for direction in 1..=2 {
                let target = current.wrapping_add(if direction == 1 { -1 } else { 1 });
                assert_eq!(
                    plan(6, &[current, count, direction]).unwrap(),
                    [
                        i32::from(if direction == 1 {
                            target >= 0
                        } else {
                            target < count
                        }),
                        target
                    ]
                );
            }
        }
    }
    for enabled in 0..=1 {
        for ready in 0..=1 {
            for fraction in [
                f32::NAN,
                -1.0,
                0.05,
                f32::from_bits(0.05_f32.to_bits() + 1),
                f32::INFINITY,
            ] {
                assert_eq!(
                    plan(8, &[enabled, ready, bits(fraction)]).unwrap(),
                    [if enabled == 0 || ready == 0 {
                        0
                    } else if fraction > 0.05 {
                        1
                    } else {
                        2
                    }]
                );
            }
        }
    }
}

#[test]
fn rejects_invalid_frames_and_units() {
    for (op, facts) in [
        (99, vec![]),
        (0, vec![3, 0, 0]),
        (0, vec![0, 0, 2]),
        (1, vec![0, -1]),
        (1, vec![0, 1]),
        (1, vec![0, 1, 65_536]),
        (7, vec![1, -1]),
        (3, vec![0, 0, 0, 0, 1]),
        (3, [vec![0; 4], vec![0; 25]].concat()),
        (3, vec![0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (3, vec![0, 0, 0, 0, 1, bits(f32::NAN), 0, 0, 0]),
    ] {
        assert!(plan(op, &facts).is_err());
    }
    assert!(plan(1, &vec![0; MAX_FACTS + 1]).is_err());
}
