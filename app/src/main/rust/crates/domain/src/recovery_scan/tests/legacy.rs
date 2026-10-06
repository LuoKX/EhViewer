use super::*;

#[derive(Clone, Copy)]
struct Child {
    source: u32,
    directory: bool,
    link: bool,
}

fn reference_walk(
    graph: &[Vec<Child>],
    backend: ScanBackend,
    max_depth: i32,
    max_nodes: i32,
    duration: i64,
) -> (Vec<(u32, u32)>, bool) {
    let mut nodes = vec![(0, 0)];
    let mut pending = VecDeque::from([(0, 0)]);
    let mut seen = HashSet::from([0]);
    let mut now = 0;
    let mut limited = false;
    while let Some(&(source, depth)) = pending.front() {
        now += 1;
        if now >= duration || i64::try_from(nodes.len()).unwrap() >= i64::from(max_nodes) {
            limited = true;
            break;
        }
        pending.pop_front();
        for child in &graph[usize::try_from(source).unwrap()] {
            now += 1;
            if now >= duration || i64::try_from(nodes.len()).unwrap() >= i64::from(max_nodes) {
                limited = true;
                break;
            }
            if backend == ScanBackend::Documents && !seen.insert(child.source) {
                continue;
            }
            if backend == ScanBackend::Physical && child.link {
                continue;
            }
            let child_depth = depth + 1;
            if child.directory && i64::from(child_depth) > i64::from(max_depth) {
                limited = true;
                continue;
            }
            nodes.push((child.source, child_depth));
            if child.directory {
                pending.push_back((child.source, child_depth));
            }
        }
    }
    (nodes, limited)
}

fn migrated_walk(
    graph: &[Vec<Child>],
    backend: ScanBackend,
    max_depth: i32,
    max_nodes: i32,
    duration: i64,
) -> (Vec<(u32, u32)>, bool) {
    let mut opts = options(backend);
    opts.max_depth = max_depth;
    opts.max_nodes = max_nodes;
    opts.started_at_millis = 0;
    opts.max_duration_millis = duration;
    if backend == ScanBackend::Documents {
        opts.root_document_id = 0_u32.to_le_bytes().to_vec().into();
    }
    let mut state = ScanState::new(opts).unwrap();
    let mut nodes = vec![(0, 0)];
    let mut now = 0;
    loop {
        now += 1;
        let Some(parent) = state.poll(now).unwrap() else {
            break;
        };
        let source = nodes[usize::try_from(parent.index).unwrap()].0;
        for child in &graph[usize::try_from(source).unwrap()] {
            now += 1;
            if !state.gate(now).unwrap() {
                break;
            }
            if backend == ScanBackend::Documents
                && !state.observe_document(&child.source.to_le_bytes()).unwrap()
            {
                continue;
            }
            let link = backend == ScanBackend::Physical && child.link;
            if let Some(ticket) = state.accept(child.directory, link).unwrap() {
                assert_eq!(usize::try_from(ticket.index).unwrap(), nodes.len());
                nodes.push((child.source, ticket.depth));
            }
        }
        state.finish_directory().unwrap();
    }
    (nodes, state.limit_exceeded())
}

fn random(seed: &mut u64, bound: u32) -> u32 {
    *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    u32::try_from((*seed >> 32) % u64::from(bound)).unwrap()
}

#[test]
fn reference_bfs_matches_1024_clocked_graphs_with_duplicates_cycles_links_and_limits() {
    let mut seed = 0x4a4e_4953_4341_4e31;
    for case in 0..512 {
        let graph: Vec<_> = (0..32)
            .map(|_| {
                (0..random(&mut seed, 9))
                    .map(|_| Child {
                        source: random(&mut seed, 32),
                        directory: random(&mut seed, 3) == 0,
                        link: random(&mut seed, 7) == 0,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let depth = i32::try_from(random(&mut seed, 6)).unwrap() - 1;
        let maximum = i32::try_from(random(&mut seed, 35)).unwrap() - 2;
        let duration = i64::from(random(&mut seed, 100));
        for backend in [ScanBackend::Documents, ScanBackend::Physical] {
            assert_eq!(
                migrated_walk(&graph, backend, depth, maximum, duration),
                reference_walk(&graph, backend, depth, maximum, duration),
                "case {case} {backend:?}"
            );
        }
    }
}
