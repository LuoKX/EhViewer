//! Download/favorite entry decisions; permissions, settings, dialogs and effects stay Android-owned.

/// Largest partition observation, including repeated gallery selections.
pub const MAX_FACTS: usize = 100_000;

/// Plans one phase using opaque platform observations.
/// 0: stable partition (count followed by existing/new slots); 1: default download
/// (direct, use default); 2: favorite entry (chooser, remove row, fetch note, slot);
/// 3: favorite selection; 4: label menu selection (minus one means default).
///
/// # Errors
/// Rejects unknown operations, inconsistent flag counts and invalid menu selections.
pub fn plan(
    operation: i32,
    left: i32,
    right: i32,
    facts: &[bool],
) -> Result<Vec<i32>, &'static str> {
    if facts.len() > MAX_FACTS {
        return Err("Entry observations exceed limit");
    }
    match (operation, facts) {
        (0, facts) => {
            let existing: Vec<i32> = facts
                .iter()
                .zip(0_i32..)
                .filter(|(v, _)| **v)
                .map(|(_, i)| i)
                .collect();
            let count =
                i32::try_from(existing.len()).map_err(|_| "Entry observations exceed limit")?;
            Ok(std::iter::once(count)
                .chain(existing)
                .chain(
                    facts
                        .iter()
                        .zip(0_i32..)
                        .filter(|(v, _)| !**v)
                        .map(|(_, i)| i),
                )
                .collect())
        }
        (
            1,
            [
                force,
                has_default,
                default_null,
                default_exists,
                empty_labels,
            ],
        ) => {
            let use_default = !force && *has_default && (*default_null || *default_exists);
            Ok(vec![
                i32::from(*force || use_default || *empty_labels),
                i32::from(use_default),
            ])
        }
        (2, [signed_in, note_missing]) => {
            let favorited = left != -2;
            let chooser = *signed_in && right == -2;
            let slot = if !signed_in {
                -1
            } else if favorited {
                -2
            } else {
                right
            };
            Ok(vec![
                i32::from(chooser),
                i32::from(chooser && favorited),
                i32::from(chooser && left >= 0 && *note_missing),
                slot,
            ])
        }
        (3, [remove_row]) if (0..if *remove_row { 12 } else { 11 }).contains(&left) => {
            Ok(vec![left - if *remove_row { 2 } else { 1 }])
        }
        (4, []) if right >= 0 && (0..=right).contains(&left) => Ok(vec![left - 1]),
        _ => Err("Invalid entry action input"),
    }
}

#[cfg(test)]
mod tests;
