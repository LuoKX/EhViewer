use super::*;

#[test]
fn exact_shared_cbor_goldens_match_kotlin() {
    use std::fmt::Write as _;
    let mut input = request(LookupKind::Lookup);
    input.token = Some("012345abcd".into());
    input.title = Some("123- Name".into());
    input.display_name = "123- Name".into();
    input.has_page_token = true;
    let hex = |bytes: Vec<u8>| {
        bytes.iter().fold(String::new(), |mut result, byte| {
            write!(result, "{byte:02x}").unwrap();
            result
        })
    };
    assert_eq!(
        hex(serde_cbor::to_vec(&input).unwrap()),
        include_str!("../../test_data/recovery_eh_lookup_request.cbor.hex").trim()
    );
    assert_eq!(
        hex(serde_cbor::to_vec(&build_envelope(&input)).unwrap()),
        include_str!("../../test_data/recovery_eh_lookup_response.cbor.hex").trim()
    );
}

use serde_json::json;
fn request(kind: LookupKind) -> LookupRequest {
    serde_json::from_value(json!({"version":1,"kind":kind})).unwrap()
}
fn built(request: &LookupRequest) -> LookupPlan {
    build_envelope(request).payload.unwrap()
}
fn candidate(gid: i64, token: &str, title: &str) -> Candidate {
    Candidate {
        gid,
        token: token.into(),
        title: Some(title.into()),
    }
}
#[test]
fn hints_and_lookup_order_keep_generated_and_stored_title_semantics() {
    let mut input = request(LookupKind::Lookup);
    assert_eq!(built(&input).steps, []);
    input.display_name = " /root/123- Generated title ".into();
    input.title = Some("123- Stored title ".into());
    input.token = Some("012345abcd".into());
    input.has_page_token = true;
    input.page_index = -2;
    let plan = built(&input);
    assert_eq!(plan.gid, Some(123));
    assert_eq!(plan.title.as_deref(), Some("Stored title"));
    assert_eq!(
        plan.steps
            .iter()
            .map(|step| step.kind.as_str())
            .collect::<Vec<_>>(),
        ["pageToken", "storedToken", "title", "gid"]
    );
    assert_eq!(plan.steps[0].key, "gtoken:123:-2");
    assert_eq!(plan.steps[2].query.as_deref(), Some("\"Stored title\""));
    input.title = Some("123-".into());
    input.token = Some("ABCDEF0123".into());
    assert_eq!(built(&input).title.as_deref(), Some("Generated title"));
    input.display_name = "123.CBZ".into();
    input.title = None;
    assert_eq!(built(&input).steps.len(), 2);
    input.has_page_token = false;
    assert_eq!(built(&input).steps[0].kind, "gid");
    input.display_name = "0001.cbz".into();
    assert_eq!(built(&input).gid, None);
    input.display_name = "9999999999999999999.cbz".into();
    assert_eq!(built(&input).gid, None);
    input.display_name.clear();
    input.gid = Some(7);
    input.title = Some("7-\"A\"\t B".into());
    let plan = built(&input);
    assert_eq!(plan.steps[0].key, "search:title:\"a\" b");
    assert_eq!(plan.steps[0].query.as_deref(), Some("\"A \t B\""));
    assert_eq!(normalize(" \tÉ TITLE\u{b}\nX\u{a0}"), "é title x");
}
#[test]
fn selectors_exact_fallback_known_ids_dedup_validity_and_top25() {
    let mut input = request(LookupKind::Select);
    input.selection = Some(SelectionKind::Title);
    input.title = Some("Name".into());
    input.candidates = vec![
        candidate(1, "0000000001", " name "),
        candidate(2, "0000000002", "Other"),
        candidate(1, "0000000001", "NAME"),
        candidate(0, "0000000003", "Name"),
        candidate(3, "ABCDEF0123", "Name"),
    ];
    assert_eq!(built(&input).selected, [0]);
    input.gid = Some(2);
    assert!(built(&input).selected.is_empty());
    input.title = Some("missing".into());
    assert_eq!(built(&input).selected, [1]);
    input.selection = Some(SelectionKind::Gid);
    assert_eq!(built(&input).selected, [1]);
    input.selection = Some(SelectionKind::Identity);
    assert_eq!(built(&input).selected, [0, 1]);
    input.candidates = (1..=30)
        .map(|gid| candidate(gid, "0000000001", "x"))
        .collect();
    assert_eq!(built(&input).selected, (0..25).collect::<Vec<_>>());
    input.selection = None;
    assert!(build_envelope(&input).error.is_some());
    // Raw observations can be shared by the HTTP cache without reusing a previous gid's selection.
    input.selection = Some(SelectionKind::Gid);
    input.gid = Some(2);
    assert_eq!(built(&input).selected, [1]);
    input.gid = Some(3);
    assert_eq!(built(&input).selected, [2]);
}
#[test]
fn apply_validates_before_hydration_and_reconciles_status_issues_evidence() {
    let mut input = request(LookupKind::ValidateIdentity);
    assert!(build_envelope(&input).error.is_some());
    input.gid = Some(7);
    input.token = Some("0000000001".into());
    input.known_gid = Some(8);
    assert!(build_envelope(&input).error.is_some());
    input.known_gid = Some(7);
    assert_eq!(built(&input).status, None);
    input.kind = LookupKind::Apply;
    input.local_pages = 3;
    input.declared_pages = 3;
    input.usable_path = true;
    input.issues =
        "MISSING_GID,MISSING_TOKEN,UNKNOWN_PAGE_COUNT,INVALID_METADATA,EH_QUERY_FAILED,EH_NO_MATCH"
            .into();
    input.evidence = "LOCAL,,LOCAL".into();
    let plan = built(&input);
    assert_eq!(plan.status.as_deref(), Some("READY"));
    assert_eq!(plan.issues.as_deref(), Some(""));
    assert_eq!(plan.evidence.as_deref(), Some("LOCAL,EH_LOOKUP"));
    input.usable_path = false;
    assert_eq!(built(&input).status.as_deref(), Some("PATH_CONFLICT"));
    input.declared_pages = 4;
    let plan = built(&input);
    assert_eq!(plan.status.as_deref(), Some("CONTENT_INCOMPLETE"));
    assert_eq!(
        plan.issues.as_deref(),
        Some("PAGE_COUNT_CONFLICT,ANDROID_PATH_CONFLICT")
    );
    input.issues = "CRC_FAILURE,CRC_FAILURE,EH_TIMEOUT".into();
    input.usable_path = true;
    input.declared_pages = 3;
    assert_eq!(built(&input).issues.as_deref(), Some("CRC_FAILURE"));
    input.local_pages = 0;
    input.declared_pages = 0;
    input.issues.clear();
    assert_eq!(built(&input).status.as_deref(), Some("CONTENT_INCOMPLETE"));
    input.token = Some("bad".into());
    assert!(build_envelope(&input).error.is_some());
}
#[test]
fn causal_classification_and_promotion_precedence_are_closed_and_stable() {
    let cases = [
        ("NotLoggedInException", "EH_AUTH_REQUIRED"),
        ("CloudflareBypassException", "EH_CLOUDFLARE_FAILED"),
        ("UnknownHostException", "EH_DNS_FAILED"),
        ("SocketTimeoutException", "EH_TIMEOUT"),
        ("HttpTimeoutException", "EH_TIMEOUT"),
        ("ConnectException", "EH_NETWORK_FAILED"),
        ("IOException", "EH_NETWORK_FAILED"),
        ("ParseException", "EH_PARSE_FAILED"),
        ("SerializationException", "EH_PARSE_FAILED"),
        ("RustFailureException", "EH_PARSE_FAILED"),
        ("EhException", "EH_RESPONSE_FAILED"),
        ("IllegalArgumentException", "EH_RESPONSE_FAILED"),
        ("IllegalStateException", "EH_RESPONSE_FAILED"),
        ("NoSuchElementException", "EH_RESPONSE_FAILED"),
        ("Unknown", "EH_QUERY_FAILED"),
    ];
    for (name, issue) in cases {
        let mut input = request(LookupKind::QueryFailure);
        input.cause_types = vec![name.into()];
        assert_eq!(built(&input).issues.as_deref(), Some(issue));
    }
}

#[test]
fn promotion_precedence_and_existing_issues_are_preserved() {
    let mut input = request(LookupKind::QueryFailure);
    input.not_found = true;
    input.issues = "CRC_FAILURE,EH_TIMEOUT".into();
    assert_eq!(
        built(&input).issues.as_deref(),
        Some("CRC_FAILURE,EH_NO_MATCH")
    );
    input.kind = LookupKind::Failure;
    input.not_found = false;
    input.cause_types = vec!["IOException".into()];
    assert!(!built(&input).promotion_failure);
    input.latest_evidence = Some("EH_LOOKUP".into());
    input.latest_issues = Some("LOCAL,EH_NO_MATCH".into());
    input.current_status = Some("READY".into());
    let plan = built(&input);
    assert!(plan.promotion_failure);
    assert_eq!(plan.status.as_deref(), Some("PATH_CONFLICT"));
    assert_eq!(plan.issues.as_deref(), Some("LOCAL,LOCAL_PROMOTION_FAILED"));
    input.current_status = Some("CONTENT_INCOMPLETE".into());
    assert_eq!(built(&input).status, input.current_status);
    input.current_status = None;
    assert_eq!(built(&input).status.as_deref(), Some("PATH_CONFLICT"));
    input.latest_evidence = None;
    input.latest_issues = None;
    for (signal, status, issue) in [
        (
            PromotionSignal::SourceMissing,
            "SOURCE_MISSING",
            "RECOVERY_SOURCE_MISSING",
        ),
        (
            PromotionSignal::IdentityConflict,
            "IDENTITY_CONFLICT",
            "IDENTITY_CONFLICT",
        ),
        (
            PromotionSignal::PageCountConflict,
            "CONTENT_INCOMPLETE",
            "PAGE_COUNT_CONFLICT",
        ),
        (
            PromotionSignal::PathConflict,
            "PATH_CONFLICT",
            "ANDROID_PATH_CONFLICT",
        ),
        (
            PromotionSignal::MappingConflict,
            "PATH_CONFLICT",
            "EXISTING_MAPPING_CONFLICT",
        ),
        (
            PromotionSignal::ActiveDownloadConflict,
            "IDENTITY_CONFLICT",
            "ACTIVE_DOWNLOAD_CONFLICT",
        ),
        (
            PromotionSignal::FormalIdentityConflict,
            "IDENTITY_CONFLICT",
            "IDENTITY_CONFLICT",
        ),
        (
            PromotionSignal::LocalPromotionFailed,
            "PATH_CONFLICT",
            "LOCAL_PROMOTION_FAILED",
        ),
    ] {
        input.promotion_signals = vec![signal];
        let plan = built(&input);
        assert_eq!(plan.status.as_deref(), Some(status));
        assert!(plan.issues.unwrap().ends_with(issue));
    }
    input.promotion_signals = vec![
        PromotionSignal::FormalIdentityConflict,
        PromotionSignal::PageCountConflict,
    ];
    assert_eq!(built(&input).status.as_deref(), Some("CONTENT_INCOMPLETE"));
    input.promotion_signals = vec![
        PromotionSignal::IdentityConflict,
        PromotionSignal::SourceMissing,
    ];
    assert_eq!(built(&input).status.as_deref(), Some("SOURCE_MISSING"));
}
#[test]
fn wire_roundtrip_and_resource_failures_do_not_echo_inputs() {
    let mut input = request(LookupKind::Lookup);
    input.display_name = "private-display-name".into();
    input.version = 2;
    assert_eq!(
        build_envelope(&input).error.unwrap(),
        RustError::Error("Invalid EH recovery input".into())
    );
    input.version = 1;
    input.cause_types = vec!["type".into(); 9];
    assert!(build_envelope(&input).error.is_some());
    input.cause_types.clear();
    input.promotion_signals = vec![PromotionSignal::SourceMissing; 9];
    assert!(build_envelope(&input).error.is_some());
    input.promotion_signals.clear();
    input.candidates = (0..1001)
        .map(|gid| candidate(gid, "token", "title"))
        .collect();
    assert!(build_envelope(&input).error.is_some());
    input.candidates.clear();
    let restored: LookupRequest =
        serde_cbor::from_slice(&serde_cbor::to_vec(&input).unwrap()).unwrap();
    let plan = built(&restored);
    let output: LookupPlan = serde_cbor::from_slice(&serde_cbor::to_vec(&plan).unwrap()).unwrap();
    assert_eq!(output, plan);
}
