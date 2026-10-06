use super::*;

mod legacy;

fn id(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}
fn options(backend: ScanBackend) -> ScanOptions {
    ScanOptions {
        version: 1,
        backend,
        max_depth: 2,
        max_nodes: MAX_NODES,
        started_at_millis: 100,
        max_duration_millis: 100,
        root_document_id: if backend == ScanBackend::Documents {
            id("root").into()
        } else {
            Vec::new().into()
        },
    }
}
fn physical() -> ScanState {
    ScanState::new(options(ScanBackend::Physical)).unwrap()
}
fn begin(state: &mut ScanState) {
    assert_eq!(
        state.poll(100).unwrap(),
        Some(ScanTicket { index: 0, depth: 0 })
    );
}
fn physical_child(state: &mut ScanState, directory: bool, link: bool) -> Option<ScanTicket> {
    assert!(state.gate(100).unwrap());
    state.accept(directory, link).unwrap()
}
fn document(state: &mut ScanState, key: &[u8], directory: bool) -> Option<ScanTicket> {
    assert!(state.gate(100).unwrap());
    if state.observe_document(key).unwrap() {
        state.accept(directory, false).unwrap()
    } else {
        None
    }
}

#[test]
fn fifo_depth_symlink_and_slot_policy_match_legacy() {
    let mut opts = options(ScanBackend::Physical);
    opts.max_depth = 1;
    let mut state = ScanState::new(opts).unwrap();
    begin(&mut state);
    assert_eq!(
        physical_child(&mut state, true, false).unwrap().packed(),
        (1_i64 << 32) | 1
    );
    assert_eq!(physical_child(&mut state, false, false).unwrap().index, 2);
    assert_eq!(physical_child(&mut state, true, true), None);
    assert_eq!(physical_child(&mut state, true, false).unwrap().index, 3);
    state.finish_directory().unwrap();
    assert_eq!(state.poll(100).unwrap().unwrap().index, 1);
    assert_eq!(physical_child(&mut state, true, false), None);
    assert!(state.limit_exceeded());
    assert_eq!(
        physical_child(&mut state, false, false),
        Some(ScanTicket { index: 4, depth: 2 })
    );
    state.finish_directory().unwrap();
    assert_eq!(state.poll(100).unwrap().unwrap().index, 3);
    state.finish_directory().unwrap();
    assert_eq!(state.poll(999).unwrap(), None);
}

#[test]
fn root_and_rejected_document_ids_remain_seen_with_raw_surrogates() {
    let mut opts = options(ScanBackend::Documents);
    opts.max_depth = 0;
    let mut state = ScanState::new(opts).unwrap();
    begin(&mut state);
    assert_eq!(document(&mut state, &id("root"), true), None);
    let lone_surrogate = [0, 0xd8];
    assert_eq!(document(&mut state, &lone_surrogate, true), None);
    assert_eq!(document(&mut state, &lone_surrogate, false), None);
    assert_eq!(document(&mut state, &id("child"), false).unwrap().index, 1);
    assert_eq!(document(&mut state, &id("child"), false), None);
    state.finish_directory().unwrap();
    assert_eq!(state.poll(100).unwrap(), None);
    assert!(state.limit_exceeded());
}

#[test]
fn exact_node_cap_is_not_exceeded_without_more_work() {
    let mut opts = options(ScanBackend::Physical);
    opts.max_nodes = 2;
    let mut state = ScanState::new(opts.clone()).unwrap();
    begin(&mut state);
    physical_child(&mut state, false, false);
    state.finish_directory().unwrap();
    assert_eq!(state.poll(200).unwrap(), None);
    assert!(!state.limit_exceeded());
    let mut state = ScanState::new(opts.clone()).unwrap();
    begin(&mut state);
    physical_child(&mut state, false, false);
    assert!(!state.gate(100).unwrap());
    state.finish_directory().unwrap();
    assert!(state.limit_exceeded());
    let mut state = ScanState::new(opts).unwrap();
    begin(&mut state);
    physical_child(&mut state, true, false);
    state.finish_directory().unwrap();
    assert_eq!(state.poll(100).unwrap(), None);
    assert!(state.limit_exceeded());
}

#[test]
fn signed_limits_deadline_equality_and_java_overflow_are_preserved() {
    for maximum in [-1, 0, 1] {
        let mut opts = options(ScanBackend::Physical);
        opts.max_nodes = maximum;
        let mut state = ScanState::new(opts).unwrap();
        assert_eq!(state.poll(100).unwrap(), None);
        assert!(state.limit_exceeded());
    }
    for duration in [-1, 0] {
        let mut opts = options(ScanBackend::Physical);
        opts.max_duration_millis = duration;
        let mut state = ScanState::new(opts).unwrap();
        assert_eq!(state.poll(100).unwrap(), None);
        assert!(state.limit_exceeded());
    }
    let mut state = physical();
    assert_eq!(state.poll(200).unwrap(), None);
    let mut state = physical();
    begin(&mut state);
    assert!(!state.gate(200).unwrap());
    state.finish_directory().unwrap();
    assert!(state.limit_exceeded());
    let mut opts = options(ScanBackend::Physical);
    opts.started_at_millis = i64::MAX;
    opts.max_duration_millis = 1;
    let mut state = ScanState::new(opts).unwrap();
    assert_eq!(state.poll(i64::MAX).unwrap(), None);
    let mut opts = options(ScanBackend::Physical);
    opts.max_depth = -1;
    let mut state = ScanState::new(opts).unwrap();
    begin(&mut state);
    assert_eq!(physical_child(&mut state, true, false), None);
    assert!(physical_child(&mut state, false, false).is_some());
}

#[test]
fn configuration_and_protocol_errors_are_closed_and_private() {
    let baseline = options(ScanBackend::Physical);
    let mut bad = baseline.clone();
    bad.version = 2;
    assert!(ScanState::new(bad).is_err());
    let mut bad = baseline.clone();
    bad.max_nodes = MAX_NODES + 1;
    assert!(ScanState::new(bad).is_err());
    let mut bad = baseline.clone();
    bad.root_document_id = vec![1].into();
    assert!(ScanState::new(bad).is_err());
    let mut bad = baseline.clone();
    bad.root_document_id = id("secret").into();
    assert!(ScanState::new(bad).is_err());
    let mut bad = baseline;
    bad.root_document_id = vec![0; MAX_ID_BYTES + 2].into();
    assert!(ScanState::new(bad).is_err());
    let mut state = physical();
    assert!(state.gate(100).is_err());
    assert!(state.accept(false, false).is_err());
    assert!(state.finish_directory().is_err());
    assert!(state.observe_document(&id("private")).is_err());
    begin(&mut state);
    assert!(state.poll(100).is_err());
    assert!(state.gate(100).unwrap());
    assert!(state.gate(100).is_err());
    assert!(state.finish_directory().is_err());
    assert!(state.observe_document(&id("private")).is_err());
    state.accept(false, false).unwrap();
    state.finish_directory().unwrap();
    let mut state = ScanState::new(options(ScanBackend::Documents)).unwrap();
    begin(&mut state);
    assert!(state.accept(false, false).is_err());
    assert!(state.gate(100).unwrap());
    assert!(state.observe_document(&[1]).is_err());
    assert!(state.observe_document(&id("key")).unwrap());
    assert!(state.observe_document(&id("key")).is_err());
    assert!(state.accept(false, true).is_err());
    state.accept(false, false).unwrap();
    state.finish_directory().unwrap();
}

#[test]
fn provider_identity_byte_and_cardinality_budgets_stop_unbounded_rejections() {
    let mut state = ScanState::new(options(ScanBackend::Documents)).unwrap();
    begin(&mut state);
    assert!(state.gate(100).unwrap());
    assert!(!state.observe_document(&vec![0; MAX_ID_BYTES + 2]).unwrap());
    assert!(!state.gate(100).unwrap());
    state.finish_directory().unwrap();
    assert_eq!(state.poll(100).unwrap(), None);
    assert!(state.limit_exceeded());
    let mut opts = options(ScanBackend::Documents);
    opts.max_depth = 0;
    let mut state = ScanState::new(opts.clone()).unwrap();
    begin(&mut state);
    for key in 0..70_u32 {
        let mut bytes = vec![0; MAX_ID_BYTES];
        bytes[..4].copy_from_slice(&key.to_le_bytes());
        if !state.gate(100).unwrap() {
            break;
        }
        if !state.observe_document(&bytes).unwrap() {
            break;
        }
        assert_eq!(state.accept(true, false).unwrap(), None);
    }
    assert!(state.stopped);
    assert!(state.seen_bytes <= MAX_SEEN_BYTES);
    state.finish_directory().unwrap();
    let mut state = ScanState::new(opts).unwrap();
    begin(&mut state);
    for key in 0..MAX_NODES as u32 {
        assert!(state.gate(100).unwrap());
        if !state.observe_document(&key.to_le_bytes()).unwrap() {
            break;
        }
        assert_eq!(state.accept(true, false).unwrap(), None);
    }
    assert!(state.stopped);
    assert_eq!(state.seen.len(), MAX_NODES as usize);
    state.finish_directory().unwrap();
}

#[test]
fn explicit_stop_and_options_serde_keep_boundary_shape() {
    let opts = options(ScanBackend::Documents);
    let bytes = serde_cbor::to_vec(&opts).unwrap();
    let decoded: ScanOptions = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(decoded.backend, ScanBackend::Documents);
    assert_eq!(decoded.root_document_id, opts.root_document_id);
    let mut state = physical();
    state.stop();
    assert_eq!(state.poll(100).unwrap(), None);
    assert!(state.limit_exceeded());
}
