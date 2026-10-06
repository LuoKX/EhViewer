use super::*;
use eh_domain::update_policy::{EMPTY_ARTIFACTS, EMPTY_RELEASE, EMPTY_WORKFLOWS, UpdateDecision};

fn policy() -> UpdatePolicyRequest {
    UpdatePolicyRequest {
        version: 1,
        operation: UpdateOperation::Check,
        now_seconds: 0,
        now_nanos: 0,
        last_seconds: 0,
        interval_days: 0,
        force_check: false,
        flavor: "default".into(),
        abis: vec!["arm64-v8a".into()],
        names: vec![],
        matches: None,
        current: "v2".into(),
        latest: String::new(),
    }
}
fn request(operation: UpdateResponseOperation, json: &str) -> UpdateRequest {
    UpdateRequest {
        version: 1,
        operation,
        policy: policy(),
        json: Some(json.into()),
    }
}
fn payload(operation: UpdateResponseOperation, json: &str) -> UpdatePlan {
    build_envelope(&request(operation, json)).payload.unwrap()
}
fn failure(operation: UpdateResponseOperation, json: &str, message: &'static str) {
    let result = build_envelope(&request(operation, json));
    assert!(result.payload.is_none());
    assert_eq!(result.error, Some(update_policy::error(message)));
}
#[test]
fn release_selects_before_comparison_even_same_version_and_preserves_downgrade_candidate() {
    for (version, decision) in [
        ("v1", UpdateDecision::Candidate),
        ("v2", UpdateDecision::NoUpdate),
        ("", UpdateDecision::Candidate),
    ] {
        let json = format!(
            r#"{{"tag_name":"{version}","body":"notes","html_url":"release","assets":[{{"name":"z-default-arm64-v8a","url":"first"}},{{"name":"a-default-arm64-v8a","url":"second"}}]}}"#
        );
        let result = payload(UpdateResponseOperation::Release, &json);
        assert_eq!(result.policy.unwrap().decision, decision);
        assert_eq!(result.download_link.as_deref(), Some("first"));
        assert_eq!(result.info.as_deref(), Some("notes"));
        assert_eq!(result.selected_index, Some(0));
        assert_eq!(result.source_count, Some(2));
    }
    failure(
        UpdateResponseOperation::Release,
        r#"{"tag_name":"v2","body":"","html_url":"","assets":[]}"#,
        EMPTY_RELEASE,
    );
}
#[test]
fn workflows_preserve_first_run_short_sha_and_absent_runs_error() {
    let json = r#"{"workflow_runs":[{"head_sha":"abcdef0123","display_title":"first","artifacts_url":"first-url"},{"head_sha":"other","display_title":"second","artifacts_url":"second-url"}]}"#;
    let mut r = request(UpdateResponseOperation::Workflow, json);
    r.policy.current = "abcdef0".into();
    let result = build_envelope(&r).payload.unwrap();
    assert_eq!(result.policy.unwrap().decision, UpdateDecision::NoUpdate);
    assert_eq!(result.workflow_run.unwrap().display_title, "first");
    r.policy.current.clear();
    assert_eq!(
        build_envelope(&r).payload.unwrap().policy.unwrap().decision,
        UpdateDecision::Candidate
    );
    for sha in ["", "abc", "123456😀"] {
        let json = format!(
            r#"{{"workflow_runs":[{{"head_sha":"{sha}","display_title":"","artifacts_url":""}}]}}"#
        );
        let result = payload(UpdateResponseOperation::Workflow, &json);
        assert_eq!(
            result.policy.unwrap().version_units.unwrap(),
            sha.encode_utf16().take(7).collect::<Vec<_>>()
        );
    }
    failure(
        UpdateResponseOperation::Workflow,
        r#"{"workflow_runs":[]}"#,
        EMPTY_WORKFLOWS,
    );
}
#[test]
fn artifacts_sort_utf16_stably_and_keep_expired_assets_with_no_match_fallback() {
    let json = r#"{"artifacts":[{"name":"\ue000","archive_download_url":"last","expired":false},{"name":"😀","archive_download_url":"first","expired":true},{"name":"😀","archive_download_url":"duplicate"}]}"#;
    let result = payload(UpdateResponseOperation::Artifacts, json);
    assert_eq!(result.download_link.as_deref(), Some("first"));
    assert_eq!(result.selected_index, Some(1));
    assert_eq!(result.policy.unwrap().index, Some(1));
    let json = r#"{"artifacts":[{"name":"z-default-arm64-v8a","archive_download_url":"last"},{"name":"a-default-arm64-v8a","archive_download_url":"first"}]}"#;
    assert_eq!(
        payload(UpdateResponseOperation::Artifacts, json)
            .download_link
            .as_deref(),
        Some("first")
    );
    failure(
        UpdateResponseOperation::Artifacts,
        r#"{"artifacts":[]}"#,
        EMPTY_ARTIFACTS,
    );
}
#[test]
fn repo_and_commits_preserve_display_facts_unknown_fields_and_last_duplicate_wins() {
    let result = payload(
        UpdateResponseOperation::Repo,
        r#"{"default_branch":"ignored","default_branch":"main","token":"not-returned"}"#,
    );
    assert_eq!(result.branch.as_deref(), Some("main"));
    let result = payload(
        UpdateResponseOperation::Comparison,
        r#"{"commits":[{"commit":{"message":"title\nbody","author":{"name":"author"},"other":null}}],"ignored":{"anything":true}}"#,
    );
    let commits = result.commits.unwrap();
    assert_eq!(commits[0].commit.message, "title\nbody");
    assert_eq!(commits[0].commit.author.name, "author");
    assert!(
        payload(UpdateResponseOperation::Comparison, r#"{"commits":[]}"#)
            .commits
            .unwrap()
            .is_empty()
    );
}
#[test]
fn malformed_json_missing_fields_wrong_types_and_deep_ignored_data_are_redacted() {
    for operation in [
        UpdateResponseOperation::Repo,
        UpdateResponseOperation::Release,
        UpdateResponseOperation::Workflow,
        UpdateResponseOperation::Artifacts,
        UpdateResponseOperation::Comparison,
    ] {
        for json in [
            "",
            "not-json-secret",
            "null",
            "[]",
            "{}",
            r#"{"default_branch":1,"assets":null,"artifacts":null,"workflow_runs":null,"commits":null}"#,
        ] {
            failure(operation, json, INVALID_RESPONSE);
        }
    }
    let deep = format!(
        r#"{{"default_branch":"main","ignored":{}0{}}}"#,
        "[".repeat(130),
        "]".repeat(130)
    );
    failure(UpdateResponseOperation::Repo, &deep, INVALID_RESPONSE);
    failure(
        UpdateResponseOperation::Release,
        r#"{"tag_name":"v1","body":"","html_url":"","assets":[{"name":"ok","url":"ok"},{"name":"bad"}]}"#,
        INVALID_RESPONSE,
    );
}
#[test]
fn all_decoded_facts_and_raw_json_have_length_count_budgets() {
    let text = "x".repeat(MAX_TEXT + 1);
    failure(
        UpdateResponseOperation::Repo,
        &format!(r#"{{"default_branch":"{text}"}}"#),
        INVALID_RESPONSE,
    );
    failure(
        UpdateResponseOperation::Repo,
        &format!(
            r#"{{"default_branch":"main","ignored":"{}"}}"#,
            "x".repeat(MAX_JSON)
        ),
        INVALID_RESPONSE,
    );
    for json in [
        format!(r#"{{"tag_name":"{text}","body":"","html_url":"","assets":[]}}"#),
        format!(r#"{{"tag_name":"v1","body":"{text}","html_url":"","assets":[]}}"#),
        format!(r#"{{"tag_name":"v1","body":"","html_url":"{text}","assets":[]}}"#),
        format!(
            r#"{{"tag_name":"v1","body":"","html_url":"","assets":[{{"name":"{text}","url":""}}]}}"#
        ),
        format!(
            r#"{{"tag_name":"v1","body":"","html_url":"","assets":[{{"name":"","url":"{text}"}}]}}"#
        ),
    ] {
        failure(UpdateResponseOperation::Release, &json, INVALID_RESPONSE);
    }
    for json in [
        format!(
            r#"{{"workflow_runs":[{{"head_sha":"{text}","display_title":"","artifacts_url":""}}]}}"#
        ),
        format!(
            r#"{{"workflow_runs":[{{"head_sha":"","display_title":"{text}","artifacts_url":""}}]}}"#
        ),
        format!(
            r#"{{"workflow_runs":[{{"head_sha":"","display_title":"","artifacts_url":"{text}"}}]}}"#
        ),
    ] {
        failure(UpdateResponseOperation::Workflow, &json, INVALID_RESPONSE);
    }
    for json in [
        format!(r#"{{"artifacts":[{{"name":"{text}","archive_download_url":""}}]}}"#),
        format!(r#"{{"artifacts":[{{"name":"","archive_download_url":"{text}"}}]}}"#),
    ] {
        failure(UpdateResponseOperation::Artifacts, &json, INVALID_RESPONSE);
    }
    for json in [
        format!(r#"{{"commits":[{{"commit":{{"message":"{text}","author":{{"name":""}}}}}}]}}"#),
        format!(r#"{{"commits":[{{"commit":{{"message":"","author":{{"name":"{text}"}}}}}}]}}"#),
    ] {
        failure(UpdateResponseOperation::Comparison, &json, INVALID_RESPONSE);
    }
    for (operation, key, item) in [
        (
            UpdateResponseOperation::Release,
            "assets",
            r#"{"name":"","url":""}"#,
        ),
        (
            UpdateResponseOperation::Workflow,
            "workflow_runs",
            r#"{"head_sha":"","display_title":"","artifacts_url":""}"#,
        ),
        (
            UpdateResponseOperation::Artifacts,
            "artifacts",
            r#"{"name":"","archive_download_url":""}"#,
        ),
        (
            UpdateResponseOperation::Comparison,
            "commits",
            r#"{"commit":{"message":"","author":{"name":""}}}"#,
        ),
    ] {
        let values = vec![item; MAX_ITEMS + 1].join(",");
        let prefix = if operation == UpdateResponseOperation::Release {
            r#""tag_name":"","body":"","html_url":"","#
        } else {
            ""
        };
        let json = format!("{{{prefix}\"{key}\":[{values}]}}");
        failure(operation, &json, INVALID_RESPONSE);
    }
}
#[test]
fn malformed_transport_version_shapes_and_policy_are_typed() {
    let mut r = request(
        UpdateResponseOperation::Repo,
        r#"{"default_branch":"main"}"#,
    );
    r.version = 2;
    assert_eq!(
        build_envelope(&r).error,
        Some(update_policy::error(update_policy::INVALID_INPUT))
    );
    r.version = 1;
    r.json = None;
    assert!(build_envelope(&r).error.is_some());
    // Defense-in-depth parse guard: malformed transport never obtains JSON.
    assert_eq!(
        parse::<Repo>(&r).err(),
        Some(update_policy::error(INVALID_RESPONSE))
    );
    assert_eq!(
        parse::<Release>(&r).err(),
        Some(update_policy::error(INVALID_RESPONSE))
    );
    assert_eq!(
        parse::<Runs>(&r).err(),
        Some(update_policy::error(INVALID_RESPONSE))
    );
    assert_eq!(
        parse::<Artifacts>(&r).err(),
        Some(update_policy::error(INVALID_RESPONSE))
    );
    assert_eq!(
        parse::<Comparison>(&r).err(),
        Some(update_policy::error(INVALID_RESPONSE))
    );
    r.operation = UpdateResponseOperation::Policy;
    r.json = Some("secret".into());
    assert!(build_envelope(&r).error.is_some());
    r.json = None;
    r.policy.version = 2;
    assert!(build_envelope(&r).error.is_some());
    r.policy.version = 1;
    assert_eq!(
        build_envelope(&r).payload.unwrap().policy.unwrap().decision,
        UpdateDecision::Skip
    );
}

#[test]
fn shared_exact_cbor_goldens_are_independently_decoded_and_reencoded() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/updater");
    let mut tested = 0;
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if !name.ends_with("_request.cbor.hex") {
            continue;
        }
        let bytes = unhex(&std::fs::read_to_string(&path).unwrap());
        let r: UpdateRequest = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(serde_cbor::to_vec(&r).unwrap(), bytes, "{name}");
        let expected = unhex(
            &std::fs::read_to_string(root.join(name.replace("_request", "_response"))).unwrap(),
        );
        let envelope = build_envelope(&r);
        assert_eq!(serde_cbor::to_vec(&envelope).unwrap(), expected, "{name}");
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<UpdatePlan>>(&expected).unwrap(),
            envelope
        );
        tested += 1;
    }
    assert_eq!(tested, 27);
}
fn unhex(value: &str) -> Vec<u8> {
    let value = value.trim();
    (0..value.len())
        .step_by(2)
        .map(|start| u8::from_str_radix(&value[start..start + 2], 16).unwrap())
        .collect()
}
