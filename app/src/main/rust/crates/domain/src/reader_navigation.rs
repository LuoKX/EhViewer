//! Discrete reader decisions, separate from decode/page-state policy and animation execution.
/// Maximum primitive observation frame (UTF-16 tags are the largest input).
pub const MAX_FACTS: usize = 1024 * 1024;
const INVALID: &str = "Invalid reader navigation input";

/// Plans one phase. Float observations and results use their raw 32-bit representation.
///
/// Operations: 0 entry; 1 mode/tags; 2 regions; 3 tap intent; 4 pan threshold;
/// 5 key; 6 page target; 7 archive sorting; 8 double-tap; 9 layout direction; 10 navigation mode.
/// Actions: 0 menu/unhandled, 1 previous, 2 next; pan side 0 none, 1 left, 2 right.
///
/// # Errors
/// Rejects malformed shapes and closed-enum drift without exposing the input.
pub fn plan(operation: i32, facts: &[i32]) -> Result<Vec<i32>, &'static str> {
    if facts.len() > MAX_FACTS {
        return Err(INVALID);
    }
    Ok(match (operation, facts) {
        (0, [kind @ 0..=2, page, archive @ 0..=1]) => vec![
            i32::from(*kind == 0 && *page == -1),
            match kind {
                0 if *archive == 0 => 0,
                0 | 1 => 1,
                _ => 2,
            },
        ],
        (1, [preference, tags @ ..]) => mode(*preference, tags)?,
        (
            2,
            [
                value,
                vertical @ 0..=1,
                horizontal_invert @ 0..=1,
                vertical_invert @ 0..=1,
            ],
        ) => regions(
            *value,
            *vertical != 0,
            *horizontal_invert != 0,
            *vertical_invert != 0,
        ),
        (3, [x, y, rtl @ 0..=1, webtoon @ 0..=1, regions @ ..]) => {
            let hit = hit_region(*x, *y, regions)?;
            tap(hit, *rtl != 0, *webtoon != 0)
        }
        (4, [available @ 0..=1, remaining]) => {
            vec![i32::from(*available != 0 && float(*remaining) > 1.0)]
        }
        (5, [key @ 0..=4, down @ 0..=1, volume @ 0..=1, inverted @ 0..=1]) => vec![if *down == 0 {
            0
        } else {
            match key {
                1 => 1,
                2 => 2,
                3 if *volume != 0 => {
                    if *inverted != 0 {
                        2
                    } else {
                        1
                    }
                }
                4 if *volume != 0 => {
                    if *inverted != 0 {
                        1
                    } else {
                        2
                    }
                }
                _ => 0,
            }
        }],
        (6, [current, count, direction @ 1..=2]) => {
            let target = if *direction == 1 {
                current.wrapping_sub(1)
            } else {
                current.wrapping_add(1)
            };
            vec![
                i32::from(if *direction == 1 {
                    target >= 0
                } else {
                    target < *count
                }),
                target,
            ]
        }
        (7, [has_info @ 0..=1, filename @ ..]) => {
            check_units(filename)?;
            vec![i32::from(
                *has_info == 0 || filename.ends_with(&[46, 122, 105, 112]),
            )]
        }
        (8, [enabled @ 0..=1, ready @ 0..=1, fraction]) => vec![if *enabled == 0 || *ready == 0 {
            0
        } else if float(*fraction) > 0.05 {
            1
        } else {
            2
        }],
        (9, [rtl @ 0..=1, layout_rtl @ 0..=1, vertical @ 0..=1]) => vec![
            i32::from(*vertical == 0 && *rtl != 0),
            i32::from(*vertical == 0 && rtl != layout_rtl),
        ],
        (10, [value, vertical @ 0..=1]) => vec![navigation_mode(*value, *vertical != 0)],
        _ => return Err(INVALID),
    })
}

fn mode(preference: i32, tags: &[i32]) -> Result<Vec<i32>, &'static str> {
    let webtoon = has_webtoon(tags)?;
    let mode = if (1..=5).contains(&preference) {
        preference
    } else if webtoon {
        4
    } else {
        2
    };
    Ok(vec![
        mode,
        i32::from(mode >= 3),
        i32::from(mode >= 4),
        i32::from(mode == 2),
    ])
}

fn hit_region(x: i32, y: i32, regions: &[i32]) -> Result<i32, &'static str> {
    if !regions.len().is_multiple_of(5) || regions.len() > 20 {
        return Err(INVALID);
    }
    let (x, y) = (float(x), float(y));
    let mut hit = 0;
    for region in regions.chunks_exact(5) {
        let [kind @ 1..=4, left, top, right, bottom] = region else {
            return Err(INVALID);
        };
        let [left, top, right, bottom] = [*left, *top, *right, *bottom].map(float);
        if ![left, top, right, bottom].iter().all(|v| v.is_finite()) {
            return Err(INVALID);
        }
        if hit == 0 && x >= left && x < right && y >= top && y < bottom {
            hit = *kind;
        }
    }
    Ok(hit)
}

fn float(bits: i32) -> f32 {
    f32::from_bits(u32::from_ne_bytes(bits.to_ne_bytes()))
}
fn bits(value: f32) -> i32 {
    i32::from_ne_bytes(value.to_bits().to_ne_bytes())
}

fn check_units(units: &[i32]) -> Result<(), &'static str> {
    if units.iter().any(|u| !(0..=65_535).contains(u)) {
        return Err(INVALID);
    }
    Ok(())
}

fn has_webtoon(mut tags: &[i32]) -> Result<bool, &'static str> {
    let mut found = false;
    while let Some((&length, rest)) = tags.split_first() {
        let length = usize::try_from(length).map_err(|_| INVALID)?;
        let Some((tag, remaining)) = rest.split_at_checked(length) else {
            return Err(INVALID);
        };
        check_units(tag)?;
        found |= tag.ends_with(&[119, 101, 98, 116, 111, 111, 110]);
        tags = remaining;
    }
    Ok(found)
}

fn tap(hit: i32, rtl: bool, webtoon: bool) -> Vec<i32> {
    if hit == 0 {
        return vec![0, 0];
    }
    if webtoon {
        return vec![if hit == 2 || hit == 4 { 2 } else { 1 }, 0];
    }
    match hit {
        1 => vec![1, if rtl { 1 } else { 2 }],
        2 => vec![2, if rtl { 2 } else { 1 }],
        3 => vec![if rtl { 2 } else { 1 }, 2],
        _ => vec![if rtl { 1 } else { 2 }, 1],
    }
}

fn navigation_mode(value: i32, vertical: bool) -> i32 {
    if (1..=5).contains(&value) {
        value
    } else if vertical {
        1
    } else {
        4
    }
}

fn regions(value: i32, vertical: bool, invert_x: bool, invert_y: bool) -> Vec<i32> {
    let value = navigation_mode(value, vertical);
    let regions: &[(i32, [f32; 4])] = match value {
        1 => &[
            (1, [0.0, 0.33, 0.33, 0.66]),
            (1, [0.0, 0.0, 1.0, 0.33]),
            (2, [0.66, 0.33, 1.0, 0.66]),
            (2, [0.0, 0.66, 1.0, 1.0]),
        ],
        2 => &[(2, [0.33, 0.33, 1.0, 1.0]), (1, [0.0, 0.33, 0.33, 1.0])],
        3 => &[
            (2, [0.0, 0.0, 0.33, 1.0]),
            (1, [0.33, 0.66, 0.66, 1.0]),
            (2, [0.66, 0.0, 1.0, 1.0]),
        ],
        4 => &[(3, [0.0, 0.0, 0.33, 1.0]), (4, [0.66, 0.0, 1.0, 1.0])],
        _ => &[],
    };
    regions
        .iter()
        .flat_map(|(kind, [left, top, right, bottom])| {
            let (left, right) = if invert_x {
                (1.0 - right, 1.0 - left)
            } else {
                (*left, *right)
            };
            let (top, bottom) = if invert_y {
                (1.0 - bottom, 1.0 - top)
            } else {
                (*top, *bottom)
            };
            [*kind, bits(left), bits(top), bits(right), bits(bottom)]
        })
        .collect()
}

#[cfg(test)]
mod tests;
