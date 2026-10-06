use super::*;
use eh_domain::recovery_scan::ScanBackend;

fn options(backend: ScanBackend) -> ScanOptions {
    ScanOptions {
        version: 1,
        backend,
        max_depth: 32,
        max_nodes: 100_000,
        started_at_millis: 0,
        max_duration_millis: 100,
        root_document_id: Vec::new().into(),
    }
}

#[test]
fn bounded_owned_handles_are_unique_and_close_reclaims_capacity() {
    let mut registry = Registry::default();
    let handles: Vec<_> = (0..MAX_SCANS)
        .map(|_| registry.create(options(ScanBackend::Physical)).unwrap())
        .collect();
    assert!(registry.create(options(ScanBackend::Physical)).is_err());
    assert!(registry.step(handles[0], 6, 1).is_err());
    assert_eq!(registry.step(handles[0], 6, 0).unwrap(), 0);
    assert!(registry.step(handles[0], 6, 0).is_err());
    let replacement = registry.create(options(ScanBackend::Physical)).unwrap();
    assert!(replacement > *handles.last().unwrap());
    assert!(registry.step(handles[0], 5, 0).is_err());
    assert!(registry.observe(handles[0], &[]).is_err());
    registry.next = i64::MAX;
    registry.step(handles[1], 6, 0).unwrap();
    assert!(registry.create(options(ScanBackend::Physical)).is_err());
    let mut invalid = options(ScanBackend::Physical);
    invalid.version = 2;
    assert!(Registry::default().create(invalid).is_err());
}

#[test]
fn scalar_protocol_covers_fifo_gates_accept_finish_limit_stop_and_invalid_calls() {
    let mut registry = Registry::default();
    let handle = registry.create(options(ScanBackend::Physical)).unwrap();
    assert_eq!(registry.step(handle, 1, 0).unwrap(), 0);
    assert_eq!(registry.step(handle, 2, 0).unwrap(), 1);
    assert_eq!(registry.step(handle, 3, 1).unwrap(), (1_i64 << 32) | 1);
    assert_eq!(registry.step(handle, 4, 0).unwrap(), 0);
    assert_eq!(registry.step(handle, 1, 0).unwrap(), (1_i64 << 32) | 1);
    assert_eq!(registry.step(handle, 4, 0).unwrap(), 0);
    assert_eq!(registry.step(handle, 1, 0).unwrap(), -1);
    assert_eq!(registry.step(handle, 5, 0).unwrap(), 0);
    for (op, arg) in [(0, 0), (3, 4), (3, -1), (4, 1), (5, 1), (7, 1), (99, 0)] {
        assert!(registry.step(handle, op, arg).is_err());
    }
    assert_eq!(registry.step(handle, 7, 0).unwrap(), 0);
    assert_eq!(registry.step(handle, 5, 0).unwrap(), 1);
    registry.step(handle, 6, 0).unwrap();
    let handle = registry.create(options(ScanBackend::Documents)).unwrap();
    registry.step(handle, 1, 0).unwrap();
    registry.step(handle, 2, 0).unwrap();
    assert!(!registry.observe(handle, &[]).unwrap());
    registry.step(handle, 2, 0).unwrap();
    assert!(registry.observe(handle, &[0, 0xd8]).unwrap());
    assert_eq!(registry.step(handle, 3, 0).unwrap(), (1_i64 << 32) | 1);
    registry.step(handle, 4, 0).unwrap();
    registry.step(handle, 6, 0).unwrap();
}

#[test]
fn global_boundary_releases_even_after_poison_and_does_not_hide_failure() {
    let handle = create(options(ScanBackend::Documents)).unwrap();
    step(handle, 1, 0).unwrap();
    step(handle, 2, 0).unwrap();
    assert!(observe(handle, &[1, 0]).unwrap());
    step(handle, 3, 0).unwrap();
    let panic = std::panic::catch_unwind(|| {
        let _guard = registry().lock().unwrap();
        panic!("test-only poison");
    });
    assert!(panic.is_err());
    assert!(create(options(ScanBackend::Physical)).is_err());
    assert!(step(handle, 5, 0).is_err());
    assert!(observe(handle, &[]).is_err());
    assert_eq!(step(handle, 6, 0).unwrap(), 0);
    registry().clear_poison();
    assert!(step(handle, 5, 0).is_err());
    let fresh = create(options(ScanBackend::Physical)).unwrap();
    step(fresh, 6, 0).unwrap();
}
