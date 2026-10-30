use super::*;
use anyhow::anyhow;

// Drive all scenarios through one production monomorph. Separate closure types
// otherwise give LLVM constant-folded instances with incompatible line counters.
fn run<'a>(
    qr: bool,
    dimensions: impl FnOnce() -> Result<(i32, i32)> + 'a,
    detect: impl FnOnce() -> Result<[i32; 4]> + 'a,
) -> Result<Option<[i32; 4]>> {
    let dimensions: Box<dyn FnOnce() -> Result<(i32, i32)> + 'a> = Box::new(dimensions);
    let detect: Box<dyn FnOnce() -> Result<[i32; 4]> + 'a> = Box::new(detect);
    plan_crop(qr, dimensions, detect)
}

#[test]
fn skips_qr_before_observations_and_elongated_images_before_detection() {
    assert_eq!(
        run(true, || panic!("dimensions read"), || panic!("pixels read")).unwrap(),
        None
    );
    for size in [(200, 100), (100, 200), (1, i32::MAX), (i32::MAX, 1)] {
        assert_eq!(
            run(false, || Ok(size), || panic!("pixels read")).unwrap(),
            None
        );
    }
}

#[test]
fn preserves_integer_ratio_and_strict_float_thresholds() {
    for (size, rect, accepted) in [
        ((100, 199), [0, 0, 76, 150], true),
        ((199, 100), [0, 0, 150, 76], true),
        ((100, 100), [0, 0, 75, 100], false),
        ((100, 100), [0, 0, 100, 75], false),
        ((100, 100), [10, 10, 86, 86], true),
        ((1, 1), [0, 0, 1, 1], true),
        ((i32::MAX, i32::MAX), [0, 0, i32::MAX, i32::MAX], true),
        ((100, 100), [i32::MIN, 0, i32::MAX, 100], false),
    ] {
        assert_eq!(
            run(false, || Ok(size), || Ok(rect)).unwrap(),
            accepted.then_some(rect)
        );
    }
}

#[test]
fn errors_propagate_without_detection_replay() {
    assert_eq!(
        run(
            false,
            || Err(anyhow!("dimension failure")),
            || panic!("pixels read")
        )
        .unwrap_err()
        .to_string(),
        "dimension failure"
    );
    for size in [(0, 1), (1, 0), (-1, 1), (1, -1)] {
        assert_eq!(
            run(false, || Ok(size), || panic!("pixels read"))
                .unwrap_err()
                .to_string(),
            "Invalid crop dimensions"
        );
    }
    assert_eq!(
        run(false, || Ok((100, 100)), || Err(anyhow!("pixel failure")))
            .unwrap_err()
            .to_string(),
        "pixel failure"
    );
}
