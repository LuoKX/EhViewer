use super::*;

#[test]
fn exact_shared_cbor_goldens_match_kotlin() {
    use std::fmt::Write as _;
    let input = EnrichmentRequest {
        version: 1,
        kind: EnrichmentKind::Language,
        title: Some("[ENG]".into()),
        tags: None,
        facts: None,
    };
    let hex = |bytes: Vec<u8>| {
        bytes.iter().fold(String::new(), |mut result, byte| {
            write!(result, "{byte:02x}").unwrap();
            result
        })
    };
    assert_eq!(
        hex(serde_cbor::to_vec(&input).unwrap()),
        include_str!("../../test_data/gallery_enrichment_request.cbor.hex").trim()
    );
    assert_eq!(
        hex(serde_cbor::to_vec(&build_envelope(&input)).unwrap()),
        include_str!("../../test_data/gallery_enrichment_response.cbor.hex").trim()
    );
}

#[test]
fn tags_and_all_title_rules_preserve_order_and_unicode_case() {
    for (index, name) in TAGS.iter().enumerate() {
        assert_eq!(
            infer_language(None, Some(&[format!("language:{name}")])),
            Some(CODES[index].to_owned())
        );
    }
    let cases = [
        "[ENG]",
        "（CHINESE）",
        "[ESPAÑOL]",
        "[KOREA]",
        "[RUS]",
        "[FR]",
        "[PORTUGUESE",
        "[THAI ภาษาไทย]",
        "[GERMAN]",
        "[ITALIANO]",
        "[VIETNAMESE Tiếng Việt]",
        "[POLISH]",
        "[HUN]",
        "[DUTCH]",
    ];
    for (index, title) in cases.iter().enumerate() {
        assert_eq!(
            infer_language(Some(title), None),
            Some(CODES[index].to_owned()),
            "{title}"
        );
    }
    for (index, title) in [
        "英訳",
        "中国翻訳",
        "スペイン翻訳",
        "韓国翻訳",
        "ロシア翻訳",
        "フランス翻訳",
        "ポルトガル翻訳",
        "タイ翻訳",
        "ドイツ翻訳",
        "イタリア翻訳",
        "ベトナム翻訳",
        "ポーランド翻訳",
        "ハンガリー翻訳",
        "オランダ翻訳",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            infer_language(Some(title), None),
            Some(CODES[index].to_owned())
        );
    }
    assert_eq!(
        infer_language(Some("[ENG] [CHINESE]"), None),
        Some("EN".to_owned())
    );
    assert_eq!(
        infer_language(
            Some("[ENG]"),
            Some(&[
                "language:unknown".into(),
                "language:chinese".into(),
                "language:english".into()
            ])
        ),
        Some("ZH".into())
    );
    assert_eq!(
        infer_language(Some("[ENG]"), Some(&["Language:chinese".into()])),
        Some("EN".into())
    );
    assert_eq!(infer_language(Some("[ENG]"), Some(&[])), Some("EN".into()));
    assert_eq!(infer_language(None, None), None);
    assert_eq!(infer_language(Some("unrecognized"), None), None);
    assert_eq!(infer_language(Some("[ſPANISH]"), None), Some("ES".into()));
    assert_eq!(infer_language(Some("[İTALIAN]"), None), Some("IT".into()));
}
#[test]
fn eligibility_exhausts_all_boolean_combinations_and_invalid_shapes() {
    for flags in 0..64 {
        let facts = MetadataFacts {
            filter: flags & 1 != 0,
            needs_tags: flags & 2 != 0,
            has_tags: flags & 4 != 0,
            show_pages: flags & 8 != 0,
            has_pages: flags & 16 != 0,
            has_rated: flags & 32 != 0,
        };
        let expected = (facts.filter && facts.needs_tags && !facts.has_tags)
            || (facts.show_pages && !facts.has_pages)
            || facts.has_rated;
        let request = EnrichmentRequest {
            version: 1,
            kind: EnrichmentKind::MetadataNeeded,
            title: None,
            tags: None,
            facts: Some(facts),
        };
        assert_eq!(
            build_envelope(&request).payload.unwrap().needs_metadata,
            Some(expected)
        );
    }
    let mut request = EnrichmentRequest {
        version: 1,
        kind: EnrichmentKind::Language,
        title: None,
        tags: None,
        facts: None,
    };
    assert_eq!(build_envelope(&request).payload.unwrap().language, None);
    request.version = 2;
    assert!(build_envelope(&request).error.is_some());
    request.version = 1;
    request.kind = EnrichmentKind::MetadataNeeded;
    assert!(build_envelope(&request).error.is_some());
    request.title = Some("private-title".into());
    assert!(build_envelope(&request).error.is_some());
    request.kind = EnrichmentKind::Language;
    request.facts = Some(MetadataFacts {
        filter: false,
        needs_tags: false,
        has_tags: false,
        show_pages: false,
        has_pages: false,
        has_rated: false,
    });
    assert_eq!(
        build_envelope(&request).error.unwrap(),
        RustError::Error("Invalid gallery enrichment shape".into())
    );
}
