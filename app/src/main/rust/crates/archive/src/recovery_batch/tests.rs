use super::*;

fn request() -> RecoveryBatchRequest {
    RecoveryBatchRequest {
        version: 3,
        start_index: 0,
        max_entries: 32,
        maximum_entries: 100_000,
        all_readable: true,
    }
}
fn context() -> RecoveryBatchContext {
    RecoveryBatchContext {
        entry_count: 2,
        encrypted: false,
        unsafe_path: false,
    }
}
fn entry(name: &str, readable: bool) -> RecoveryBatchEntry {
    RecoveryBatchEntry {
        name: name.into(),
        size: 4,
        readable,
    }
}

#[test]
fn plans_short_batches_and_preserves_cumulative_failure() {
    let first = finish_recovery_batch(context(), &request(), vec![entry("1.jpg", false)]).unwrap();
    assert_eq!(first.next_index, 1);
    assert!(!first.complete && !first.all_readable);
    let next = RecoveryBatchRequest {
        start_index: first.next_index,
        all_readable: first.all_readable,
        ..request()
    };
    assert_eq!(plan_recovery_batch(context(), &next).unwrap(), 1..2);
    let last = finish_recovery_batch(context(), &next, vec![entry("2.jpg", true)]).unwrap();
    assert!(!last.complete && !last.all_readable);
    assert_eq!(last.next_index, 2);
    let valid = finish_recovery_batch(
        context(),
        &request(),
        vec![entry("1.jpg", true), entry("2.jpg", true)],
    )
    .unwrap();
    assert!(valid.complete && valid.all_readable);
}

#[test]
fn encrypted_and_count_limited_archives_need_no_decoding() {
    let locked = RecoveryBatchContext {
        entry_count: 0,
        encrypted: true,
        unsafe_path: true,
    };
    assert_eq!(plan_recovery_batch(locked, &request()).unwrap(), 0..0);
    let result = finish_recovery_batch(locked, &request(), vec![]).unwrap();
    assert_eq!(result.issues, [RecoveryBatchIssue::EncryptedArchive]);
    assert!(!result.complete);
    let limited = RecoveryBatchRequest {
        maximum_entries: 1,
        ..request()
    };
    assert_eq!(plan_recovery_batch(context(), &limited).unwrap(), 0..0);
    let result = finish_recovery_batch(context(), &limited, vec![]).unwrap();
    assert_eq!(result.issues, [RecoveryBatchIssue::LimitExceeded]);
    assert!(!result.complete);
}

#[test]
fn unsafe_path_is_published_only_at_the_final_batch() {
    let facts = RecoveryBatchContext {
        unsafe_path: true,
        ..context()
    };
    let partial = finish_recovery_batch(facts, &request(), vec![entry("1.jpg", true)]).unwrap();
    assert!(partial.issues.is_empty());
    let final_request = RecoveryBatchRequest {
        start_index: 1,
        ..request()
    };
    let last = finish_recovery_batch(facts, &final_request, vec![entry("2.jpg", true)]).unwrap();
    assert_eq!(last.issues, [RecoveryBatchIssue::UnsafeArchivePath]);
    assert!(last.complete);
}

#[test]
fn rejects_versions_bounds_and_initial_state_without_input_in_errors() {
    for invalid in [
        RecoveryBatchRequest {
            version: 2,
            ..request()
        },
        RecoveryBatchRequest {
            max_entries: 0,
            ..request()
        },
        RecoveryBatchRequest {
            max_entries: 33,
            ..request()
        },
        RecoveryBatchRequest {
            maximum_entries: 0,
            ..request()
        },
        RecoveryBatchRequest {
            start_index: 3,
            ..request()
        },
        RecoveryBatchRequest {
            all_readable: false,
            ..request()
        },
    ] {
        assert_eq!(
            plan_recovery_batch(context(), &invalid),
            Err(RecoveryBatchError::InvalidRequest)
        );
    }
    for invalid in [
        RecoveryBatchContext {
            entry_count: 0,
            encrypted: false,
            unsafe_path: false,
        },
        RecoveryBatchContext {
            encrypted: true,
            ..context()
        },
    ] {
        assert_eq!(
            plan_recovery_batch(invalid, &request()),
            Err(RecoveryBatchError::InvalidRequest)
        );
    }
    assert!(
        finish_recovery_batch(
            context(),
            &RecoveryBatchRequest {
                version: 1,
                ..request()
            },
            vec![]
        )
        .is_err()
    );
    assert_eq!(
        RecoveryBatchError::InvalidRequest.to_string(),
        "archive recovery batch request is invalid"
    );
    assert_eq!(
        RecoveryBatchError::InvalidEntries.to_string(),
        "archive recovery batch observations are invalid"
    );
}

#[test]
fn rejects_non_progress_and_invalid_entry_facts() {
    for invalid in [
        vec![],
        vec![
            entry("1.jpg", true),
            entry("2.jpg", true),
            entry("3.jpg", true),
        ],
        vec![entry("", true)],
        vec![entry(&"x".repeat(MAX_RECOVERY_BATCH_NAME_BYTES + 1), true)],
        vec![RecoveryBatchEntry {
            size: 0,
            ..entry("1.jpg", true)
        }],
        vec![RecoveryBatchEntry {
            size: 512 * 1024 * 1024 + 1,
            ..entry("1.jpg", true)
        }],
    ] {
        assert_eq!(
            finish_recovery_batch(context(), &request(), invalid),
            Err(RecoveryBatchError::InvalidEntries)
        );
    }
    let final_request = RecoveryBatchRequest {
        start_index: 2,
        ..request()
    };
    assert!(
        finish_recovery_batch(context(), &final_request, vec![])
            .unwrap()
            .complete
    );
    let maximum = RecoveryBatchContext {
        entry_count: u32::MAX,
        ..context()
    };
    let near_end = RecoveryBatchRequest {
        start_index: u32::MAX - 1,
        maximum_entries: u32::MAX,
        ..request()
    };
    assert_eq!(
        plan_recovery_batch(maximum, &near_end).unwrap(),
        u32::MAX - 1..u32::MAX
    );
}

#[test]
fn kotlin_request_and_response_goldens_pin_every_wire_field() {
    let bytes = hex(include_str!(
        "../../test_data/recovery_batch_request.cbor.hex"
    ));
    assert_eq!(
        serde_cbor::from_slice::<RecoveryBatchRequest>(&bytes).unwrap(),
        request()
    );
    assert_eq!(serde_cbor::to_vec(&request()).unwrap(), bytes);
    let payload = finish_recovery_batch(
        context(),
        &request(),
        vec![entry("1.jpg", true), entry("2.jpg", true)],
    )
    .unwrap();
    let bytes = hex(include_str!(
        "../../test_data/recovery_batch_response.cbor.hex"
    ));
    assert_eq!(serde_cbor::to_vec(&payload).unwrap(), bytes);
    assert_eq!(
        serde_cbor::from_slice::<RecoveryBatch>(&bytes).unwrap(),
        payload
    );
    for issue in [
        RecoveryBatchIssue::EncryptedArchive,
        RecoveryBatchIssue::LimitExceeded,
        RecoveryBatchIssue::UnsafeArchivePath,
    ] {
        let bytes = serde_cbor::to_vec(&issue).unwrap();
        assert_eq!(
            serde_cbor::from_slice::<RecoveryBatchIssue>(&bytes).unwrap(),
            issue
        );
    }
}

fn hex(text: &str) -> Vec<u8> {
    let text = text.trim();
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
        .collect()
}
