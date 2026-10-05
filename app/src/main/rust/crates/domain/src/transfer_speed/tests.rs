use super::*;

#[test]
fn empty_minimum_window_reset_and_stalls() {
    assert!(SpeedWindow::new(0).is_err());
    let mut window = SpeedWindow::new(1_000_000_000).unwrap();
    assert_eq!(window.sample(0).unwrap(), None);
    window.track(0, 100).unwrap();
    assert_eq!(window.sample(0).unwrap(), Some(10_000));
    assert_eq!(window.sample(1_000_000_000).unwrap(), Some(100));
    assert_eq!(window.sample(1_000_000_001).unwrap(), Some(0));
    window.reset();
    assert_eq!(window.sample(2_000_000_000).unwrap(), None);
}

#[test]
fn duplicate_timestamps_cutoff_large_totals_and_clock_regression() {
    let mut window = SpeedWindow::new(2_000_000_000).unwrap();
    window.track(100, 5).unwrap();
    window.track(100, 10).unwrap();
    assert_eq!(window.sample(100).unwrap(), Some(1_000));
    assert_eq!(window.sample(99).unwrap(), Some(1_000));
    window.track(2_000_000_100, 90).unwrap();
    assert_eq!(window.sample(2_000_000_100).unwrap(), Some(50));
    window.track(2_000_000_101, i32::MAX).unwrap();
    assert_eq!(window.received.len(), 2);
    assert_eq!(
        window.sample(4_000_000_101).unwrap(),
        Some(i64::from(i32::MAX) / 2)
    );
    assert!(window.track(0, -1).is_err());
}

#[test]
fn prunes_even_without_collection_and_caps_samples() {
    let mut window = SpeedWindow::new(10_000_000).unwrap();
    for now in 0..65_536 {
        window.track(now, 1).unwrap();
    }
    assert!(window.track(65_536, 1).is_err());
    window.track(0, 2).unwrap();
    window.track(20_000_000, 3).unwrap();
    assert_eq!(window.received.len(), 1);
    assert_eq!(window.sample(20_000_000).unwrap(), Some(300));
}
