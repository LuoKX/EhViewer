// Copyright 2021 Peter Trifanov.
// Copyright 2009-2017 java-diff-utils.
// SPDX-License-Identifier: Apache-2.0
// Adapted Myers path/tie-breaking from kotlin-multiplatform-diff-jvm 1.3.0.
//! Bounded legacy-compatible UTF-16 edit classification, without text logging.

use super::TextEdit;

const NONE: usize = usize::MAX;
const MAX_WORK: usize = 1_000_000;
const MAX_NODES: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum EditResult {
    Original,
    Target,
    Splice(TextEdit),
}

struct Node {
    i: i32,
    j: i32,
    snake: bool,
    bootstrap: bool,
    previous: Option<usize>,
}
struct Arena {
    nodes: Vec<Node>,
    limit: usize,
}
impl Arena {
    fn push(
        &mut self,
        i: i32,
        j: i32,
        snake: bool,
        bootstrap: bool,
        previous: Option<usize>,
    ) -> Option<usize> {
        if self.nodes.len() >= self.limit {
            return None;
        }
        let previous = if snake {
            previous
        } else {
            previous.and_then(|n| self.previous_snake(n))
        };
        let index = self.nodes.len();
        self.nodes.push(Node {
            i,
            j,
            snake,
            bootstrap,
            previous,
        });
        Some(index)
    }
    fn previous_snake(&self, mut index: usize) -> Option<usize> {
        loop {
            let node = &self.nodes[index];
            if node.bootstrap {
                return None;
            }
            if !node.snake
                && let Some(prev) = node.previous
            {
                index = prev;
                continue;
            }
            return Some(index);
        }
    }
}

pub(super) fn classify(source: &[u16], target: &[u16]) -> EditResult {
    bounded(source, target, MAX_WORK, MAX_NODES)
}
fn bounded(source: &[u16], target: &[u16], mut work: usize, max_nodes: usize) -> EditResult {
    if source == target {
        return EditResult::Original;
    }
    let source_size = i32::try_from(source.len()).expect("bounded source length");
    let target_size = i32::try_from(target.len()).expect("bounded target length");
    let max = source_size + target_size + 1;
    let middle = max;
    let mut diagonal = vec![NONE; usize::try_from(1 + 2 * max).expect("bounded diagonal")];
    let mut arena = Arena {
        nodes: Vec::new(),
        limit: max_nodes,
    };
    let Some(seed) = arena.push(0, -1, true, true, None) else {
        return EditResult::Target;
    };
    diagonal[usize::try_from(middle + 1).unwrap()] = seed;
    for d in 0..max {
        for k in (-d..=d).step_by(2) {
            if work == 0 {
                return EditResult::Target;
            }
            work -= 1;
            let index = usize::try_from(middle + k).unwrap();
            let plus = diagonal[index + 1];
            let minus = diagonal[index - 1];
            let previous = if k == -d || (k != d && arena.nodes[minus].i < arena.nodes[plus].i) {
                plus
            } else {
                minus
            };
            let mut i = arena.nodes[previous].i + i32::from(previous == minus);
            let mut j = i - k;
            diagonal[index - 1] = NONE;
            let Some(mut node) = arena.push(i, j, false, false, Some(previous)) else {
                return EditResult::Target;
            };
            let start = i;
            while i < source_size && j < target_size {
                if work == 0 {
                    return EditResult::Target;
                }
                work -= 1;
                if source[usize::try_from(i).unwrap()] != target[usize::try_from(j).unwrap()] {
                    break;
                }
                i += 1;
                j += 1;
            }
            if i != start {
                let Some(snake) = arena.push(i, j, true, false, Some(node)) else {
                    return EditResult::Target;
                };
                node = snake;
            }
            diagonal[index] = node;
            if i >= source_size && j >= target_size {
                return revision(&arena, node);
            }
        }
        diagonal[usize::try_from(middle + d - 1).unwrap()] = NONE;
    }
    EditResult::Target
}

fn revision(arena: &Arena, finish: usize) -> EditResult {
    let mut cursor = if arena.nodes[finish].snake {
        arena.nodes[finish].previous
    } else {
        Some(finish)
    };
    let mut edit = None;
    while let Some(index) = cursor {
        let node = &arena.nodes[index];
        let Some(previous) = node.previous else {
            break;
        };
        let anchor = &arena.nodes[previous];
        if anchor.j < 0 {
            break;
        }
        if edit.is_some() || node.snake || (node.i != anchor.i && node.j != anchor.j) {
            return EditResult::Target;
        }
        edit = Some(TextEdit {
            source_start: u32::try_from(anchor.i).unwrap(),
            source_end: u32::try_from(node.i).unwrap(),
            target_start: u32::try_from(anchor.j).unwrap(),
            target_end: u32::try_from(node.j).unwrap(),
        });
        cursor = if anchor.snake {
            anchor.previous
        } else {
            Some(previous)
        };
    }
    edit.map_or(EditResult::Original, EditResult::Splice)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(hex: &str) -> Vec<u16> {
        hex.as_bytes()
            .chunks_exact(4)
            .map(|p| u16::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn pinned_legacy_diff_positions_are_exact_including_surrogates_and_repetitions() {
        for line in include_str!("test_data/legacy-diff.tsv").lines() {
            let fields = line.split('\t').collect::<Vec<_>>();
            let actual = classify(&units(fields[0]), &units(fields[1]));
            let expected = if fields[2] == "0" {
                EditResult::Original
            } else if fields[2] == "1" && matches!(fields[3], "INSERT" | "DELETE") {
                EditResult::Splice(TextEdit {
                    source_start: fields[4].parse().unwrap(),
                    source_end: fields[5].parse().unwrap(),
                    target_start: fields[6].parse().unwrap(),
                    target_end: fields[7].parse().unwrap(),
                })
            } else {
                EditResult::Target
            };
            assert_eq!(actual, expected, "{line}");
        }
    }
    #[test]
    fn resource_exhaustion_returns_target_not_a_partial_edit() {
        for (work, nodes) in [(0, 1), (100, 0), (100, 1), (100, 2), (1, 100)] {
            assert_eq!(
                bounded(&[1, 2, 3], &[4, 5, 6], work, nodes),
                EditResult::Target
            );
        }
        assert_eq!(bounded(&[1], &[1], 0, 0), EditResult::Original);
        let arena = Arena {
            nodes: vec![
                Node {
                    i: 0,
                    j: 0,
                    snake: false,
                    bootstrap: false,
                    previous: None,
                },
                Node {
                    i: 1,
                    j: 0,
                    snake: false,
                    bootstrap: false,
                    previous: Some(0),
                },
            ],
            limit: 2,
        };
        assert_eq!(arena.previous_snake(1), Some(0));
    }
}
