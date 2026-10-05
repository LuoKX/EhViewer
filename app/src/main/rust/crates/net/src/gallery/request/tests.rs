use super::*;

#[test]
fn exact_shared_cbor_goldens_match_kotlin() {
    use std::fmt::Write as _;
    let mut input = request(RequestKind::Rate);
    input.rating = 2.01;
    let hex = |bytes: Vec<u8>| {
        bytes.iter().fold(String::new(), |mut result, byte| {
            write!(result, "{byte:02x}").unwrap();
            result
        })
    };
    assert_eq!(
        hex(serde_cbor::to_vec(&input).unwrap()),
        include_str!("../../../test_data/gallery_request_request.cbor.hex").trim()
    );
    assert_eq!(
        hex(serde_cbor::to_vec(&build_envelope(&input)).unwrap()),
        include_str!("../../../test_data/gallery_request_response.cbor.hex").trim()
    );
}

fn request(kind: RequestKind) -> GalleryRequest {
    serde_json::from_value(json!({"version":1,"site":"public","kind":kind,"gid":7,"token":"abc"}))
        .unwrap()
}
fn built(request: &GalleryRequest) -> GalleryRequestPlan {
    build_envelope(request).payload.unwrap()
}
#[test]
fn urls_keep_signed_indices_page_wrap_and_order() {
    let mut input = request(RequestKind::DetailUrl);
    assert_eq!(built(&input).url, "https://e-hentai.org/g/7/abc");
    input.index = -2;
    input.all_comments = true;
    assert_eq!(built(&input).url, "https://e-hentai.org/g/7/abc?p=-2&hc=1");
    input.index = 0;
    assert_eq!(built(&input).url, "https://e-hentai.org/g/7/abc?hc=1");
    input.kind = RequestKind::PageUrl;
    input.index = i32::MAX;
    input.gid = -7;
    assert_eq!(
        built(&input).url,
        "https://e-hentai.org/s/abc/-7--2147483648"
    );
    input.nl = Some("  ".into());
    assert!(!built(&input).url.contains('?'));
    input.nl = Some("a b+&*~".into());
    assert!(built(&input).url.ends_with("?nl=a+b%2B%26%2A~"));
    input.kind = RequestKind::MpvUrl;
    input.site = GallerySite::ExHentai;
    assert_eq!(built(&input).url, "https://exhentai.org/mpv/-7/abc");
    input.token = "a b/?#[];%".into();
    assert_eq!(
        built(&input).url,
        "https://exhentai.org/mpv/-7/a%20b%2F%3F%23%5B%5D;%25"
    );
}
#[test]
fn comments_keep_exact_text_field_order_and_optional_shapes() {
    let mut input = request(RequestKind::Comment);
    assert!(build_envelope(&input).error.is_some());
    input.url = Some("https://e-hentai.org/g/7/abc".into());
    assert!(build_envelope(&input).error.is_some());
    input.comment = Some(" x\n\"y\" ".into());
    let plan = built(&input);
    assert_eq!(plan.form, [field("commenttext_new", " x\n\"y\" ")]);
    assert_eq!(plan.referer, input.url);
    input.comment_id = Some(0);
    assert_eq!(
        built(&input).form,
        [
            field("commenttext_edit", " x\n\"y\" "),
            field("edit_comment", "0")
        ]
    );
    input.comment_id = Some(-1);
    assert_eq!(built(&input).form.len(), 1);
}
#[test]
fn showpage_rating_and_credentials_preserve_legacy_numeric_semantics() {
    let mut input = request(RequestKind::ShowPage);
    assert_eq!(built(&input).referer, None);
    assert_eq!(
        built(&input).body.as_deref(),
        Some(r#"{"method":"showpage","gid":7,"page":1,"imgkey":"abc","showkey":null}"#)
    );
    input.index = 2;
    input.previous_token = Some("prev".into());
    input.show_key = Some("key".into());
    assert_eq!(
        built(&input).referer.as_deref(),
        Some("https://e-hentai.org/s/prev/7-2")
    );
    input.previous_token = None;
    assert_eq!(built(&input).referer, None);
    input.kind = RequestKind::Rate;
    input.rating = 2.01;
    let plan = built(&input);
    assert!(plan.credentials);
    assert_eq!(
        plan.body.as_deref(),
        Some(r#"{"method":"rategallery","gid":7,"token":"abc","rating":5}"#)
    );
    for (rating, expected) in [
        (f32::NAN, 0),
        (f32::INFINITY, i32::MAX),
        (f32::NEG_INFINITY, i32::MIN),
        (-0.0, 0),
        (-1.2, -2),
        (5.0, 10),
    ] {
        assert_eq!(rating_steps(rating), expected);
    }
}
#[test]
fn all_api_shapes_are_ordered_credential_free_and_batch_bounded() {
    let mut input = request(RequestKind::Metadata);
    input.identities = vec![
        RequestIdentity {
            gid: -1,
            token: "a b".into(),
        },
        RequestIdentity {
            gid: 7,
            token: "abc".into(),
        },
    ];
    input.url = Some("referer".into());
    assert_eq!(
        built(&input).body.as_deref(),
        Some(r#"{"method":"gdata","gidlist":[[-1,"a b"],[7,"abc"]],"namespace":1}"#)
    );
    assert_eq!(built(&input).referer, input.url);
    input.identities = (0..251)
        .map(|gid| RequestIdentity {
            gid,
            token: String::new(),
        })
        .collect();
    assert!(build_envelope(&input).error.is_some());
    input.identities.clear();
    input.kind = RequestKind::VoteComment;
    assert!(build_envelope(&input).error.is_some());
    input.comment_id = Some(-8);
    input.vote = -1;
    assert_eq!(
        built(&input).body.as_deref(),
        Some(r#"{"method":"votecomment","gid":7,"token":"abc","comment_id":-8,"comment_vote":-1}"#)
    );
    input.kind = RequestKind::VoteTag;
    input.tags = "f:a,b".into();
    assert_eq!(
        built(&input).body.as_deref(),
        Some(r#"{"method":"taggallery","gid":7,"token":"abc","tags":"f:a,b","vote":-1}"#)
    );
    assert!(built(&input).credentials);
    input.kind = RequestKind::Token;
    input.index = i32::MAX;
    input.site = GallerySite::ExHentai;
    assert_eq!(
        built(&input).body.as_deref(),
        Some(r#"{"method":"gtoken","pagelist":[[7,"abc",-2147483648]]}"#)
    );
    assert_eq!(built(&input).url, "https://s.exhentai.org/api.php");
    input.version = 2;
    assert_eq!(
        build_envelope(&input).error.unwrap(),
        RustError::Error("Unsupported gallery request version".into())
    );
}
#[test]
fn cbor_contract_and_uri_alphabets_are_stable() {
    let input = request(RequestKind::Rate);
    let decoded: GalleryRequest =
        serde_cbor::from_slice(&serde_cbor::to_vec(&input).unwrap()).unwrap();
    let plan = built(&decoded);
    let restored: GalleryRequestPlan =
        serde_cbor::from_slice(&serde_cbor::to_vec(&plan).unwrap()).unwrap();
    assert_eq!(restored, plan);
    assert_eq!(encode(":@!$&'()*+,;=-._~", true), ":@!$&'()*+,;=-._~");
    assert_eq!(encode("中文", false), "%E4%B8%AD%E6%96%87");
}
