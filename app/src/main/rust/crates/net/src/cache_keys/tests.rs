use super::*;
fn request(kind: CacheKeyKind, input: Option<&str>) -> CacheKeyRequest {
    CacheKeyRequest {
        version: 1,
        kind,
        input: input.map(str::to_owned),
        gid: Some(-7),
        index: Some(-1),
        ex_hentai: Some(true),
    }
}
fn value(request: &CacheKeyRequest) -> String {
    build_envelope(request).payload.unwrap().value
}
#[test]
fn image_and_ordered_thumb_prefixes() {
    assert_eq!(value(&request(CacheKeyKind::Image, None)), "image:-7:-1");
    for (url, expected) in [
        ("https://ehgt.org/abc.jpg", "abc.jpg"),
        ("https://s.exhentai.org/t/abc.jpg", "abc.jpg"),
        ("https://s.exhentai.org/abc.webp", "abc.webp"),
        ("unknown", "unknown"),
        (
            "https://ehgt.org/https://s.exhentai.org/t/https://s.exhentai.org/x",
            "x",
        ),
    ] {
        assert_eq!(value(&request(CacheKeyKind::Thumb, Some(url))), expected);
    }
}
#[test]
fn timed_previews_and_site_url_edges() {
    for (url, expected) in [
        ("https://x.hath.network/cm/token/7-2.webp", "$m/7-2"),
        ("https://x/c1/token/7-2.jpg", "$1/7-2"),
        ("https://x/c2/token/7-2.jpg", "$2/7-2"),
        ("https://x/c2//7-2.jpg", "$https://x/c2//7-2.jpg"),
        ("https://x/c3/t/7-2.jpg", "$https://x/c3/t/7-2.jpg"),
        ("https://x/cm/t/7-2extra", "$m/7-2"),
    ] {
        assert_eq!(value(&request(CacheKeyKind::Preview, Some(url))), expected);
    }
    for (input, ex, expected) in [
        ("https:opaque", true, "https:opaque"),
        ("x.webp", true, "https://s.exhentai.org/x.webp"),
        ("x.jpg", true, "https://s.exhentai.org/t/x.jpg"),
        ("x.WEBP", true, "https://s.exhentai.org/t/x.WEBP"),
        ("", false, "https://ehgt.org/"),
        ("x.webp", false, "https://ehgt.org/x.webp"),
    ] {
        assert_eq!(
            value(&CacheKeyRequest {
                ex_hentai: Some(ex),
                ..request(CacheKeyKind::Url, Some(input))
            }),
            expected
        );
    }
}
#[test]
fn versions_missing_facts_size_and_wire() {
    for request in [
        CacheKeyRequest {
            version: 2,
            ..request(CacheKeyKind::Thumb, Some("x"))
        },
        CacheKeyRequest {
            gid: None,
            ..request(CacheKeyKind::Image, None)
        },
        CacheKeyRequest {
            index: None,
            ..request(CacheKeyKind::Image, None)
        },
        CacheKeyRequest {
            ex_hentai: None,
            ..request(CacheKeyKind::Url, Some("x"))
        },
        request(CacheKeyKind::Thumb, None),
        request(CacheKeyKind::Thumb, Some(&"x".repeat(16 * 1024 + 1))),
    ] {
        assert!(!build_envelope(&request).is_ok());
    }
    let request = request(CacheKeyKind::Preview, Some("/cm/token/7-2.webp"));
    let request: CacheKeyRequest =
        serde_cbor::from_slice(&serde_cbor::to_vec(&request).unwrap()).unwrap();
    let response = build_envelope(&request);
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<CacheKeyPlan>>(
            &serde_cbor::to_vec(&response).unwrap()
        )
        .unwrap(),
        response
    );
}
