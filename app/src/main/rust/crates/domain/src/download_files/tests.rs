use super::*;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn naming_preserves_sign_width_wrap_locale_and_raw_extension() {
    for (index, expected) in [
        (0, "00000001.jpg"),
        (-1, "00000000.jpg"),
        (-2, "-0000001.jpg"),
        (99_999_999, "100000000.jpg"),
        (i32::MAX, "-2147483648.jpg"),
        (i32::MIN, "-2147483647.jpg"),
    ] {
        assert_eq!(
            page_filename(index, 48, &units("jpg")).unwrap(),
            units(expected)
        );
    }
    assert_eq!(page_filename(0, 0x660, &[]).unwrap(), units("٠٠٠٠٠٠٠١."));
    let result = page_filename(11, 0xe50, &[0xd800, 0xdc01, 0xd800]).unwrap();
    assert_eq!(&result[..9], units("๐๐๐๐๐๐๑๒."));
    assert_eq!(&result[9..], [0xd800, 0xdc01, 0xd800]);
    assert!(page_filename(0, u16::MAX, &[]).is_err());
    assert!(page_filename(0, 48, &vec![0; MAX_EXTENSION + 1]).is_err());
    assert_eq!(
        page_filename(0, u16::MAX - 9, &vec![0; MAX_EXTENSION])
            .unwrap()
            .len(),
        MAX_EXTENSION + 9
    );
}

#[test]
fn hash_grammar_is_first_substring_lowercase_and_not_host_or_end_anchored() {
    let hash = "0123456789abcdef0123456789abcdef01234567";
    for url in [
        format!("/h/{hash}"),
        format!("prefix/h/{hash}f/trailer"),
        format!("/h/invalid/h/{hash}/h/{}", "a".repeat(40)),
    ] {
        assert_eq!(
            expected_image_hash(&units(&url)).unwrap().as_deref(),
            Some(hash)
        );
    }
    for url in [
        String::new(),
        "/H/".to_owned() + hash,
        "/h/".to_owned() + &hash[..39],
        "/h/".to_owned() + &"A".repeat(40),
    ] {
        assert_eq!(expected_image_hash(&units(&url)).unwrap(), None);
    }
    let mut raw = vec![0xd800];
    raw.extend(units(&format!("/h/{hash}")));
    assert_eq!(expected_image_hash(&raw).unwrap().as_deref(), Some(hash));
    assert_eq!(expected_image_hash(&vec![0; MAX_URL]).unwrap(), None);
    assert!(expected_image_hash(&vec![0; MAX_URL + 1]).is_err());
}

#[test]
fn cleanup_preserves_provider_order_and_only_observed_matches() {
    assert!(post_archive_pages(&[]).unwrap().is_empty());
    assert_eq!(
        post_archive_pages(&[false, true, false, true]).unwrap(),
        [1, 3]
    );
    assert!(post_archive_pages(&[false; 3]).unwrap().is_empty());
    assert_eq!(
        post_archive_pages(&vec![true; MAX_FILES]).unwrap().last(),
        Some(&99_999)
    );
    assert!(post_archive_pages(&vec![false; MAX_FILES + 1]).is_err());
}
