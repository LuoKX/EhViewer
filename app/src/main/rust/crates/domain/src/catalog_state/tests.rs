use super::*;

#[test]
fn shared_navigation_wire_golden() {
    let decode = |hex: &str| {
        hex.trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let input = decode(include_str!(
        "../../test_data/catalog_state_request.cbor.hex"
    ));
    let request: Request = serde_cbor::from_slice(&input).unwrap();
    assert_eq!(serde_cbor::to_vec(&request).unwrap(), input);
    assert_eq!(
        serde_cbor::to_vec(&build_envelope(&request)).unwrap(),
        decode(include_str!(
            "../../test_data/catalog_state_response.cbor.hex"
        ))
    );
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "fixture helper for nullable protocol fields"
)]
fn text(s: &str) -> Option<ByteBuf> {
    Some(ByteBuf::from(
        s.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    ))
}
fn request(action: Transition) -> Request {
    Request {
        version: 1,
        state: Navigation {
            previous: text("p"),
            next: text("n"),
            seek: text("s"),
            range: 23,
        },
        action,
        text: None,
        value: 0,
        forward: false,
    }
}
fn apply(r: &Request) -> Navigation {
    build_envelope(r).payload.unwrap()
}

#[test]
fn cursor_range_date_and_page_preserve_legacy_field_ownership() {
    let mut r = request(Transition::Index);
    r.text = text("");
    assert_eq!(
        apply(&r),
        Navigation {
            previous: text(""),
            ..Navigation::default()
        }
    );
    r.forward = true;
    assert_eq!(
        apply(&r),
        Navigation {
            next: text(""),
            ..Navigation::default()
        }
    );
    r.text = None;
    assert_eq!(apply(&r), Navigation::default());
    r = request(Transition::Range);
    r.value = i32::MIN;
    assert_eq!(
        apply(&r),
        Navigation {
            range: i32::MIN,
            ..Navigation::default()
        }
    );
    r = request(Transition::Seek);
    r.text = text("2026-10-29");
    assert_eq!(
        apply(&r),
        Navigation {
            next: text("2"),
            seek: r.text.clone(),
            ..Navigation::default()
        }
    );
    r = request(Transition::Page);
    for (page, seek) in [
        (1, None),
        (2, text("1")),
        (0, text("-1")),
        (i32::MIN, text("2147483647")),
        (i32::MAX, text("2147483646")),
    ] {
        r.value = page;
        assert_eq!(
            apply(&r),
            Navigation {
                seek,
                ..r.state.clone()
            }
        );
    }
}

#[test]
fn saved_progress_uses_last_code_unit_at_sign_and_preserves_surrogates() {
    let mut r = request(Transition::SavedProgress);
    for (name, suffix) in [
        ("name", None),
        ("x@", None),
        ("@12", text("12")),
        ("x@12@3", text("3")),
        ("", None),
    ] {
        r.text = text(name);
        assert_eq!(
            apply(&r),
            Navigation {
                next: suffix,
                ..r.state.clone()
            }
        );
    }
    r.text = Some(ByteBuf::from(vec![0, 0xd8, b'@', 0, 0, 0xdc]));
    assert_eq!(apply(&r).next.unwrap().as_ref(), &[0, 0xdc]);
}

#[test]
fn invalid_versions_shapes_text_sizes_and_cbor_reject() {
    let mut r = request(Transition::Page);
    r.version = 2;
    assert!(build_envelope(&r).error.is_some());
    r.version = 1;
    r.text = text("sensitive");
    assert!(build_envelope(&r).error.is_some());
    r = request(Transition::Index);
    r.value = 1;
    assert!(build_envelope(&r).error.is_some());
    r = request(Transition::Seek);
    assert!(build_envelope(&r).error.is_some());
    r.text = text("");
    r.forward = true;
    assert!(build_envelope(&r).error.is_some());
    r.forward = false;
    for size in [1, MAX_TEXT_BYTES + 2] {
        r.text = Some(ByteBuf::from(vec![0; size]));
        assert!(build_envelope(&r).error.is_some());
    }
    r.text = Some(ByteBuf::from(vec![0; MAX_TEXT_BYTES]));
    assert!(build_envelope(&r).payload.is_some());
    let bytes = serde_cbor::to_vec(&r).unwrap();
    assert_eq!(serde_cbor::from_slice::<Request>(&bytes).unwrap(), r);
    assert!(serde_cbor::from_slice::<Request>(&[0xff]).is_err());
}

#[test]
fn numeric_controls_cover_signed_wrap_and_real_validation_boundaries() {
    assert_eq!(numbers(0, -1, 512).unwrap(), vec![-513]);
    for (input, output) in [(-1, 0), (0, 0), (1, 0), (5, 4), (i32::MIN, i32::MAX)] {
        assert_eq!(numbers(1, input, 0).unwrap(), vec![output]);
    }
    for (input, output) in [(0, 0), (1, 2), (i32::MAX, i32::MIN)] {
        assert_eq!(numbers(2, input, 0).unwrap(), vec![output]);
    }
    for (op, max) in [(3, 1000), (4, 2000)] {
        for value in [i32::MIN, -1, 0, 1, max, max + 1, i32::MAX] {
            assert_eq!(numbers(op, value, 0).unwrap(), vec![value.clamp(0, max)]);
        }
    }
    for (min, max, error) in [
        (1000, 0, 0),
        (0, 9, 1),
        (0, 10, 0),
        (1, 20, 2),
        (1, 21, 0),
        (20, 40, 0),
        (21, 40, 2),
        (0, -1, 1),
    ] {
        assert_eq!(numbers(5, min, max).unwrap(), vec![error]);
    }
    for page in [-1, 0, 1, 2, 199, 200, 201, i32::MIN, i32::MAX] {
        assert_eq!(
            numbers(6, page, 0).unwrap(),
            vec![i32::from((1..=200).contains(&page))]
        );
        let prev = page.wrapping_sub(1);
        let next = page.wrapping_add(1);
        assert_eq!(
            numbers(7, page, 0).unwrap(),
            vec![i32::from(prev > 0), prev, i32::from(next <= 200), next]
        );
        assert_eq!(numbers(8, page, 0).unwrap(), vec![next]);
    }
    assert_eq!(numbers(9, 0, 0).unwrap(), vec![200]);
    assert_eq!(numbers(10, 4, 0).unwrap(), vec![0]);
    assert_eq!(numbers(10, 0, 0).unwrap(), vec![1]);
    assert!(numbers(-1, 0, 0).is_err());
}
