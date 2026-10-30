use super::*;

fn observations(text: &str) -> (Vec<u16>, Vec<u8>) {
    let text: Vec<_> = text.encode_utf16().collect();
    let facts = text
        .iter()
        .map(|unit| {
            if (48..=57).contains(unit) {
                unit.to_le_bytes()[0] - b'0' + 16
            } else {
                15
            }
        })
        .collect();
    (text, facts)
}

#[test]
fn links_match_host_path_port_greediness_and_utf16_positions() {
    for url in [
        "http://a.b",
        "https://a.b/",
        "https://a.b/a?x=1&b=%20#c",
        "http://a.b:12345/[]()_~-!@$',;=+*",
        "http://a.b.c",
        "http://a.b:12",
    ] {
        let text = format!("😀 {url} 中文");
        let (text, facts) = observations(&text);
        assert_eq!(
            links(&text, &facts).unwrap(),
            [3, i32::try_from(url.len()).unwrap() + 3]
        );
    }
    for (text, expected) in [
        ("HTTP://a.b", vec![]),
        ("http://a", vec![]),
        ("http://.a.b", vec![]),
        ("http://a..b", vec![]),
        ("xhttp://a.b:123456/end", vec![1, 17]),
        ("http://a.b:bad", vec![0, 10]),
        ("http://a.b.", vec![0, 10]),
        ("http://a.b/ http://c.d", vec![0, 11, 12, 22]),
    ] {
        let (text, facts) = observations(text);
        assert_eq!(links(&text, &facts).unwrap(), expected);
    }
}

#[test]
fn platform_digit_observations_support_supplementary_ports_without_replacing_text() {
    let (mut text, mut facts) = observations("http://a.b:");
    for _ in 0..6 {
        text.extend([0xd835, 0xdfce]);
        facts.extend([31, 31]);
    }
    assert_eq!(links(&text, &facts).unwrap(), [0, 21]);
    let (mut text, mut facts) = observations("http://a.b:");
    text.extend([0xd800, 0x31]);
    facts.extend([31, 17]);
    assert_eq!(links(&text, &facts).unwrap(), [0, 13]);
    facts[11] = 15;
    assert_eq!(links(&text, &facts).unwrap(), [0, 10]);
}

#[test]
fn image_capture_and_signed_target_arithmetic_preserve_existing_rules() {
    for (source, expected) in [
        ("a-b-100-200-c_401.jpg", Some([100, 200, 401, 600])),
        ("a-b-0-0-c_+11", Some([0, 0, 11, 15])),
        ("a-b-1-2-c_-11.jpg", Some([1, 2, -11, -15])),
        ("a-b-1-2-c_not-a-number.jpg", Some([1, 2, 200, 300])),
        (
            "a-b-1-2-c_2147483647.jpg",
            Some([1, 2, i32::MAX, -1_073_741_827]),
        ),
        (
            "a-b-1-2-c_-2147483648.jpg",
            Some([1, 2, i32::MIN, 1_073_741_824]),
        ),
        ("a-b-1-2-c_2147483648.jpg", Some([1, 2, 200, 300])),
        ("other.jpg", None),
    ] {
        let (text, facts) = observations(source);
        assert_eq!(
            image_dimensions(&text, &facts).unwrap(),
            expected,
            "{source}"
        );
    }
    for number in ["2147483648", "4294967299", "10000000000"] {
        let (text, facts) = observations(&format!("a-b-{number}-1-c_200"));
        assert_eq!(image_dimensions(&text, &facts), Err(DisplayError::Number));
        let (text, facts) = observations(&format!("a-b-1-{number}-c_200"));
        assert_eq!(image_dimensions(&text, &facts), Err(DisplayError::Number));
    }
    let (mut text, mut facts) = observations("a-b-1-2-c_3");
    text[4] = 0x661;
    facts[4] = 17;
    text[10] = 0x663;
    facts[10] = 3;
    assert_eq!(image_dimensions(&text, &facts).unwrap(), Some([1, 2, 3, 3]));
    text[4] = 0xd800;
    facts[4] = 31;
    assert_eq!(image_dimensions(&text, &facts), Err(DisplayError::Number));
    assert_eq!(int_value(&[], &[]), None);
    assert_eq!(int_value(&[43], &[15]), None);
}

#[test]
fn invalid_shapes_and_budgets_fail_without_observation_contents() {
    for (text, facts) in [
        (vec![0], vec![]),
        (vec![0], vec![32]),
        (vec![0], vec![10]),
        (vec![0; MAX_TEXT + 1], vec![15; MAX_TEXT + 1]),
    ] {
        assert_eq!(links(&text, &facts), Err(DisplayError::Input));
        assert_eq!(image_dimensions(&text, &facts), Err(DisplayError::Input));
    }
    assert!(links(&[], &[]).unwrap().is_empty());
    assert_eq!(
        DisplayError::Input.message(),
        "Invalid comment text observations"
    );
    assert_eq!(
        DisplayError::Number.message(),
        "Invalid comment image dimension"
    );
}
