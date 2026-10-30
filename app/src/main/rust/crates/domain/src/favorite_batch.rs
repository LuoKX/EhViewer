//! Cross-store favorite entry choices; HTTP, Room and refresh execution stay Android-owned.

/// Fixed V1 intent codes shared with the Android executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Action {
    /// Same source and destination: no work and no refresh.
    None = 0,
    /// Delete only the local records.
    DeleteLocal = 1,
    /// Delete cloud records using the existing bulk HTTP planner.
    DeleteCloud = 2,
    /// Upload every record first, then remove local records after complete success.
    UploadThenRemoveLocal = 3,
    /// Save a local copy, retaining cloud records.
    SaveLocal = 4,
    /// Move between cloud categories.
    MoveCloud = 5,
}

/// V1 primitive contract: delete, move, menu selection, or local-overlay eligibility.
/// Delete/move return `[action, refresh]`; selection returns one category;
/// overlay returns `[queryLocalPresence, slotToSetIfPresent]`.
///
/// # Errors
/// Rejects unknown operations and invalid destination/menu observations.
pub fn plan(operation: i32, left: i32, right: i32) -> Result<Vec<i32>, &'static str> {
    match operation {
        0 if right == 0 => Ok(intent(if left == -2 {
            Action::DeleteLocal
        } else {
            Action::DeleteCloud
        })),
        1 if right == -2 || (0..=9).contains(&right) => Ok(intent(if left == right {
            Action::None
        } else if left == -2 {
            Action::UploadThenRemoveLocal
        } else if right == -2 {
            Action::SaveLocal
        } else {
            Action::MoveCloud
        })),
        2 if (1..=11).contains(&right) && (0..right).contains(&left) => {
            Ok(vec![if left == 0 { -2 } else { left - 1 }])
        }
        3 if right == 0 => Ok(vec![i32::from(left == -2), -1]),
        _ => Err("Invalid favorite batch input"),
    }
}

fn intent(action: Action) -> Vec<i32> {
    vec![action as i32, i32::from(action != Action::None)]
}

#[cfg(test)]
mod tests;
