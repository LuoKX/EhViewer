use super::*;

fn request(kind: FileRuleKind, input: Option<&str>) -> FileRulesRequest {
    FileRulesRequest {
        version: 1,
        kind,
        units: input.map(|v| v.encode_utf16().collect()),
    }
}
fn run(kind: FileRuleKind, input: Option<&str>) -> Option<String> {
    build_envelope(&request(kind, input))
        .payload
        .unwrap()
        .units
        .map(|v| String::from_utf16_lossy(&v))
}
#[test]
fn sanitizes_invalid_characters_and_only_trailing_spaces_and_dots() {
    assert_eq!(
        run(FileRuleKind::Sanitize, Some("  a:/\\*?<>|\"\u{0}\u{7f}. ")).as_deref(),
        Some("  a___________")
    );
    assert_eq!(
        run(FileRuleKind::Sanitize, Some("中😀\t X")),
        Some("中😀_ X".into())
    );
    assert_eq!(
        run(FileRuleKind::Sanitize, Some(&"a".repeat(200))),
        Some("a".repeat(200))
    );
    assert_eq!(
        run(FileRuleKind::Sanitize, Some(&"a".repeat(201)))
            .unwrap()
            .len(),
        200
    );
}
#[test]
fn rejects_empty_missing_oversized_and_versioned_input() {
    for name in ["", " ", "...  "] {
        assert_eq!(
            build_envelope(&request(FileRuleKind::Sanitize, Some(name))).error,
            Some(RustError::Error("Invalid filename".into()))
        );
    }
    assert!(
        build_envelope(&request(FileRuleKind::Sanitize, None))
            .error
            .is_some()
    );
    let mut input = request(FileRuleKind::Basename, None);
    input.version = 2;
    assert!(build_envelope(&input).error.is_some());
    input.version = 1;
    input.units = Some(vec![65; MAX_INPUT_UNITS + 1]);
    assert!(build_envelope(&input).error.is_some());
}
#[test]
fn extracts_last_dot_and_null_empty_hidden_filename_semantics() {
    for (input, extension, basename) in [
        (None, None, None),
        (Some(""), None, None),
        (Some(".jpg"), Some("jpg"), None),
        (Some("a.tar.gz"), Some("gz"), Some("a.tar")),
        (Some("a."), None, Some("a")),
        (Some("a"), None, Some("a")),
        (Some("a . JPG"), Some(" JPG"), Some("a ")),
    ] {
        assert_eq!(run(FileRuleKind::Extension, input).as_deref(), extension);
        assert_eq!(run(FileRuleKind::Basename, input).as_deref(), basename);
    }
}
// Frozen old JVM middle-delete loop; parity exists only in offline tests.
fn legacy(name: &[u16]) -> Vec<u16> {
    let mut result = name.to_vec();
    let bytes = |v: &[u16]| {
        char::decode_utf16(v.iter().copied())
            .map(|c| c.map_or(1, char::len_utf8))
            .sum::<usize>()
    };
    if bytes(&result) > 200 {
        while bytes(&result) > 197 {
            result.remove(result.len() / 2);
        }
        result.splice(result.len() / 2..result.len() / 2, [46, 46, 46]);
    }
    result
}
#[test]
fn linear_shortening_preserves_java_surrogate_replacement_and_deletions() {
    for input in [
        "😀".repeat(200),
        "中".repeat(200),
        "é".repeat(201),
        "a😀中é".repeat(100),
    ] {
        let units = input.encode_utf16().collect::<Vec<_>>();
        assert_eq!(shorten(&units), legacy(&units));
    }
    let mut seed = 17u32;
    for count in 0..512 {
        let mut units = Vec::new();
        for _ in 0..count {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            units.push(
                [65, 0x800, 0x7ff, 0xd800, 0xdfff, 0xdbff, 0xdc00]
                    [usize::try_from(seed % 7).unwrap()],
            );
        }
        assert_eq!(shorten(&units), legacy(&units));
    }
}
#[test]
fn preserves_unpaired_utf16_in_all_operations() {
    let input = FileRulesRequest {
        version: 1,
        kind: FileRuleKind::Extension,
        units: Some(vec![0xd800, 46, 0xdfff]),
    };
    assert_eq!(
        build_envelope(&input).payload.unwrap().units,
        Some(vec![0xdfff])
    );
    let input = FileRulesRequest {
        kind: FileRuleKind::Basename,
        ..input
    };
    assert_eq!(
        build_envelope(&input).payload.unwrap().units,
        Some(vec![0xd800])
    );
}
