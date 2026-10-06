use super::*;

fn request(operation: UpdateOperation) -> UpdatePolicyRequest {
    UpdatePolicyRequest {
        version: 1,
        operation,
        now_seconds: 0,
        now_nanos: 0,
        last_seconds: 0,
        interval_days: 0,
        force_check: false,
        flavor: "default".into(),
        abis: vec!["arm64-v8a".into()],
        names: vec![],
        matches: None,
        current: String::new(),
        latest: String::new(),
    }
}
#[test]
fn clock_preserves_force_zero_interval_strict_boundary_regression_and_saturation() {
    let mut r = request(UpdateOperation::Check);
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    r.force_check = true;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Check);
    r.force_check = false;
    r.interval_days = 1;
    r.now_seconds = 86_400;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    r.now_nanos = 1;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Check);
    r.now_seconds = -1;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    r.interval_days = -1;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Check);
    for last in [i64::MAX, MAX_SECOND] {
        r.last_seconds = last;
        r.interval_days = i32::MAX;
        r.now_seconds = MAX_SECOND;
        r.now_nanos = 999_999_999;
        assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    }
    r.last_seconds = i64::MIN;
    r.interval_days = i32::MIN;
    r.now_seconds = MIN_SECOND;
    r.now_nanos = 0;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    r.now_nanos = 1;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Check);
    r.last_seconds = i64::MAX;
    r.interval_days = -1;
    r.now_seconds = MAX_SECOND - 86_400;
    r.now_nanos = 999_999_999;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Skip);
    r.now_seconds += 1;
    assert_eq!(plan(&r).unwrap().decision, UpdateDecision::Check);
}
#[test]
fn abi_only_uses_first_supported_entry_and_case_sensitive_substrings() {
    let mut r = request(UpdateOperation::Match);
    for abi in ["arm64-v8a", "x86_64", "armeabi-v7a", "x86", "unknown", ""] {
        r.abis = vec![abi.into(), "arm64-v8a".into()];
        let binary_abi = if matches!(abi, "arm64-v8a" | "x86_64" | "armeabi-v7a") {
            abi
        } else {
            "universal"
        };
        r.names = vec![format!("prefix-default-{binary_abi}-suffix")];
        assert_eq!(plan(&r).unwrap().matched, Some(true));
        r.names = vec![format!("Default-{binary_abi}")];
        assert_eq!(plan(&r).unwrap().matched, Some(false));
        r.flavor.clear();
        assert_eq!(plan(&r).unwrap().matched, Some(true));
        r.flavor = "default".into();
    }
    r.abis.clear();
    r.names = vec!["default-universal".into()];
    assert_eq!(plan(&r).unwrap().matched, Some(true));
    r.abis = vec!["unknown".into(), "arm64-v8a".into()];
    r.names = vec!["default-arm64-v8a".into()];
    assert_eq!(plan(&r).unwrap().matched, Some(false));
}
#[test]
fn selections_preserve_source_order_stable_utf16_sort_and_first_item_fallback() {
    let mut r = request(UpdateOperation::SelectRelease);
    r.names = vec!["z-default-arm64-v8a".into(), "a-default-arm64-v8a".into()];
    assert_eq!(plan(&r).unwrap().index, Some(0));
    r.operation = UpdateOperation::SelectArtifacts;
    assert_eq!(plan(&r).unwrap().index, Some(1));
    r.flavor = "missing".into();
    assert_eq!(plan(&r).unwrap().index, Some(1));
    r.operation = UpdateOperation::SelectRelease;
    assert_eq!(plan(&r).unwrap().index, Some(0));
    r.names = vec!["\u{e000}".into(), "😀".into(), "😀".into(), "a".into()];
    r.operation = UpdateOperation::PrepareArtifacts;
    assert_eq!(plan(&r).unwrap().order, Some(vec![3, 1, 2, 0]));
    r.operation = UpdateOperation::PrepareRelease;
    assert_eq!(plan(&r).unwrap().order, Some(vec![0, 1, 2, 3]));
    r.operation = UpdateOperation::SelectWorkflow;
    assert_eq!(plan(&r).unwrap().index, Some(0));
}
#[test]
fn custom_predicates_require_continuous_lazy_prefix_and_terminal_true_or_full_false() {
    let mut r = request(UpdateOperation::SelectArtifacts);
    r.names = vec!["z".into(), "a".into(), "m".into()];
    for (values, index) in [
        (vec![true], 1),
        (vec![false, true], 2),
        (vec![false, false, true], 0),
        (vec![false, false, false], 1),
    ] {
        r.matches = Some(values);
        assert_eq!(plan(&r).unwrap().index, Some(index));
    }
    for values in [
        vec![],
        vec![false],
        vec![true, false],
        vec![true, true],
        vec![false; 4],
    ] {
        r.matches = Some(values);
        assert_eq!(plan(&r).unwrap_err(), error(INVALID_INPUT));
    }
    r.matches = Some(vec![true]);
    for operation in [
        UpdateOperation::PrepareRelease,
        UpdateOperation::PrepareArtifacts,
        UpdateOperation::SelectWorkflow,
        UpdateOperation::Match,
    ] {
        r.operation = operation;
        assert_eq!(plan(&r).unwrap_err(), error(INVALID_INPUT));
    }
}
#[test]
fn comparisons_are_string_inequality_with_utf16_sha_take_seven() {
    let mut r = request(UpdateOperation::CompareRelease);
    for (current, latest, decision) in [
        ("v2", "v1", UpdateDecision::Candidate),
        ("", "", UpdateDecision::NoUpdate),
        ("1", "1", UpdateDecision::NoUpdate),
        ("v1", "1", UpdateDecision::Candidate),
    ] {
        r.current = current.into();
        r.latest = latest.into();
        assert_eq!(plan(&r).unwrap().decision, decision);
    }
    r.operation = UpdateOperation::CompareWorkflow;
    for (current, latest, decision) in [
        ("abcdef0", "abcdef012345", UpdateDecision::NoUpdate),
        ("abc", "abc", UpdateDecision::NoUpdate),
        ("", "", UpdateDecision::NoUpdate),
        ("abcdef01", "abcdef012345", UpdateDecision::Candidate),
    ] {
        r.current = current.into();
        r.latest = latest.into();
        assert_eq!(plan(&r).unwrap().decision, decision);
        assert_eq!(
            plan(&r).unwrap().version_units.unwrap(),
            latest.encode_utf16().take(7).collect::<Vec<_>>()
        );
    }
    r.latest = "123456😀".into();
    assert_eq!(
        plan(&r).unwrap().version_units.unwrap(),
        vec![49, 50, 51, 52, 53, 54, 0xd83d]
    );
}
#[test]
fn empty_lists_keep_repaired_errors_and_never_become_no_update() {
    for (operation, message) in [
        (UpdateOperation::SelectRelease, EMPTY_RELEASE),
        (UpdateOperation::PrepareRelease, EMPTY_RELEASE),
        (UpdateOperation::SelectArtifacts, EMPTY_ARTIFACTS),
        (UpdateOperation::PrepareArtifacts, EMPTY_ARTIFACTS),
        (UpdateOperation::SelectWorkflow, EMPTY_WORKFLOWS),
    ] {
        assert_eq!(plan(&request(operation)).unwrap_err(), error(message));
    }
}
#[test]
fn malformed_budget_version_and_enum_errors_are_fixed_and_roundtrips_cover_every_operation() {
    let base = request(UpdateOperation::Check);
    let mut wrong = base.clone();
    wrong.version = 2;
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    let mut wrong = base.clone();
    wrong.now_nanos = 1_000_000_000;
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    for second in [MIN_SECOND - 1, MAX_SECOND + 1] {
        let mut wrong = base.clone();
        wrong.now_seconds = second;
        assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    }
    let mut wrong = base.clone();
    wrong.names = vec![String::new(); MAX_ITEMS + 1];
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    let mut wrong = base.clone();
    wrong.abis = vec![String::new(); 33];
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    for field in 0..5 {
        let mut wrong = base.clone();
        let text = "secret".repeat(MAX_TEXT);
        match field {
            0 => wrong.flavor = text,
            1 => wrong.current = text,
            2 => wrong.latest = text,
            3 => wrong.names = vec![text],
            _ => wrong.abis = vec![text],
        }
        assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    }
    let mut wrong = request(UpdateOperation::Match);
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    wrong.names = vec!["a".into(), "b".into()];
    assert_eq!(plan(&wrong).unwrap_err(), error(INVALID_INPUT));
    let operations = [
        UpdateOperation::Check,
        UpdateOperation::Match,
        UpdateOperation::PrepareRelease,
        UpdateOperation::PrepareArtifacts,
        UpdateOperation::SelectRelease,
        UpdateOperation::SelectArtifacts,
        UpdateOperation::SelectWorkflow,
        UpdateOperation::CompareRelease,
        UpdateOperation::CompareWorkflow,
    ];
    for operation in operations {
        let mut r = request(operation);
        r.names = vec!["default-arm64-v8a".into()];
        let p = plan(&r).unwrap();
        assert_eq!(
            serde_cbor::from_slice::<UpdatePolicyRequest>(&serde_cbor::to_vec(&r).unwrap())
                .unwrap(),
            r
        );
        assert_eq!(
            serde_cbor::from_slice::<UpdatePolicyPlan>(&serde_cbor::to_vec(&p).unwrap()).unwrap(),
            p
        );
    }
    let value = serde_json::to_string(&base)
        .unwrap()
        .replace("\"check\"", "\"secret-invalid-enum\"");
    assert!(serde_json::from_str::<UpdatePolicyRequest>(&value).is_err());
}
