//! Download thumbnail phases. Cache misses still reach cleanup; exceptions never advance a phase.
const INVALID: &str = "Invalid thumbnail flow input";
/// Maximum primitive frame, including raw UTF-16 and JVM whitespace observations.
pub const MAX_FACTS: usize = 1024 * 1024;

/// Plans activation, key update, filename suffix, local/remote results and post-write cleanup.
/// Actions: 0 remote request, 1 local decode, 2 return result, 3 cache write.
///
/// # Errors
/// Rejects unsupported phases, malformed code units or oversized input without echoing data.
pub fn plan(operation: i32, facts: &[i32]) -> Result<Vec<i32>, &'static str> {
    if facts.len() > MAX_FACTS {
        return Err(INVALID);
    }
    Ok(match (operation, facts) {
        (0, [info @ 0..=1, blank @ 0..=1]) => vec![i32::from(*info == 1 && *blank == 0)],
        (1, [same_key @ 0..=1]) => vec![1 - same_key],
        (2, [length, rest @ ..]) => {
            let length = usize::try_from(*length).map_err(|_| INVALID)?;
            let Some((key, whitespace)) = rest.split_at_checked(length) else {
                return Err(INVALID);
            };
            if rest.iter().any(|u| !(0..=65_535).contains(u)) {
                return Err(INVALID);
            }
            let whitespace: std::collections::HashSet<_> = whitespace.iter().collect();
            let start = key.iter().rposition(|u| *u == 46).map_or(length, |i| i + 1);
            if key[start..].iter().all(|u| whitespace.contains(u)) {
                vec![-1]
            } else {
                vec![i32::try_from(start).map_err(|_| INVALID)?]
            }
        }
        (3, [file @ 0..=1]) => vec![*file, 0],
        (4, [success @ 0..=1, different @ 0..=1]) => vec![
            if *success == 1 { 2 } else { 0 },
            i32::from(*success == 1 && *different == 1),
        ],
        (5, [success @ 0..=1, directory @ 0..=1]) => vec![
            if *success == 1 && *directory == 1 {
                3
            } else {
                2
            },
            0,
        ],
        (6, [different @ 0..=1]) => vec![2, *different],
        _ => return Err(INVALID),
    })
}

#[cfg(test)]
mod tests;
