use std::fmt::Write as _;

use crate::contracts::ApiEnvelope;

use super::{
    DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, DownloadCommand, DownloadEntry,
    DownloadQueueError, DownloadQueuePlan, DownloadQueueRecoveryRequest, DownloadQueueRequest,
    DownloadQueueSnapshot, DownloadState, DownloadStateChange, recover_download_queue,
    reduce_download_queue,
};

fn entry(gallery_id: i64, state: DownloadState) -> DownloadEntry {
    DownloadEntry { gallery_id, state }
}
fn snapshot(
    entries: Vec<DownloadEntry>,
    waiting: Vec<i64>,
    current: Option<i64>,
) -> DownloadQueueSnapshot {
    DownloadQueueSnapshot {
        entries,
        waiting,
        current,
    }
}
fn reduce(snapshot: DownloadQueueSnapshot, command: DownloadCommand) -> DownloadQueuePlan {
    reduce_download_queue(snapshot, command).expect("test command should reduce")
}

#[test]
fn range_start_preserves_selection_order_and_advances_idle_queue() {
    let plan = reduce(
        snapshot(
            vec![
                entry(1, DownloadState::None),
                entry(2, DownloadState::Failed),
                entry(3, DownloadState::Finished),
            ],
            vec![],
            None,
        ),
        DownloadCommand::StartRange {
            gallery_ids: vec![2, 99, 1, 3],
        },
    );
    assert_eq!(plan.start_gallery_id, Some(2));
    assert_eq!(plan.snapshot.current, Some(2));
    assert_eq!(plan.snapshot.waiting, vec![1, 3]);
    assert_eq!(
        plan.snapshot.entries,
        vec![
            entry(1, DownloadState::Waiting),
            entry(2, DownloadState::Downloading),
            entry(3, DownloadState::Waiting)
        ]
    );
    assert_eq!(
        plan.changes,
        vec![
            DownloadStateChange {
                gallery_id: 1,
                previous: DownloadState::None,
                current: DownloadState::Waiting
            },
            DownloadStateChange {
                gallery_id: 2,
                previous: DownloadState::Failed,
                current: DownloadState::Downloading
            },
            DownloadStateChange {
                gallery_id: 3,
                previous: DownloadState::Finished,
                current: DownloadState::Waiting
            },
        ]
    );
}

#[test]
fn single_start_is_idempotent_for_current_and_accepts_invalid_or_waiting_entries() {
    let active = snapshot(vec![entry(1, DownloadState::Downloading)], vec![], Some(1));
    let unchanged = reduce(active.clone(), DownloadCommand::StartOne { gallery_id: 1 });
    assert_eq!(unchanged.snapshot, active);
    let waiting = reduce(
        snapshot(vec![entry(2, DownloadState::Waiting)], vec![2], None),
        DownloadCommand::StartOne { gallery_id: 2 },
    );
    assert_eq!(waiting.start_gallery_id, Some(2));
    let invalid = reduce(
        snapshot(vec![entry(3, DownloadState::Invalid)], vec![], None),
        DownloadCommand::StartOne { gallery_id: 3 },
    );
    assert_eq!(
        invalid.snapshot.entries,
        vec![entry(3, DownloadState::Downloading)]
    );
}

#[test]
fn start_all_uses_entry_order_and_excludes_finished_or_invalid_tasks() {
    let plan = reduce(
        snapshot(
            vec![
                entry(4, DownloadState::Finished),
                entry(3, DownloadState::Failed),
                entry(2, DownloadState::Invalid),
                entry(1, DownloadState::None),
            ],
            vec![],
            None,
        ),
        DownloadCommand::StartAll,
    );
    assert_eq!(plan.start_gallery_id, Some(3));
    assert_eq!(plan.snapshot.waiting, vec![1]);
    assert_eq!(plan.snapshot.entries[0].state, DownloadState::Finished);
    assert_eq!(plan.snapshot.entries[2].state, DownloadState::Invalid);
}

#[test]
fn start_commands_append_without_disturbing_an_active_fifo() {
    let plan = reduce(
        snapshot(
            vec![
                entry(1, DownloadState::Downloading),
                entry(2, DownloadState::Waiting),
                entry(3, DownloadState::None),
            ],
            vec![2],
            Some(1),
        ),
        DownloadCommand::StartRange {
            gallery_ids: vec![3],
        },
    );
    assert_eq!(plan.start_gallery_id, None);
    assert_eq!(plan.snapshot.current, Some(1));
    assert_eq!(plan.snapshot.waiting, vec![2, 3]);
}

#[test]
fn stopping_waiting_and_active_tasks_preserves_fifo_semantics() {
    let active = snapshot(
        vec![
            entry(1, DownloadState::Downloading),
            entry(2, DownloadState::Waiting),
            entry(3, DownloadState::Waiting),
        ],
        vec![2, 3],
        Some(1),
    );
    let without_waiting = reduce(active, DownloadCommand::StopOne { gallery_id: 2 });
    assert_eq!(without_waiting.snapshot.waiting, vec![3]);
    let advanced = reduce(
        without_waiting.snapshot,
        DownloadCommand::StopOne { gallery_id: 1 },
    );
    assert_eq!(advanced.cancel_gallery_id, Some(1));
    assert_eq!(advanced.start_gallery_id, Some(3));
    assert_eq!(advanced.snapshot.current, Some(3));
}

#[test]
fn stop_range_removes_every_selection_before_advancing_once() {
    let plan = reduce(
        snapshot(
            vec![
                entry(1, DownloadState::Downloading),
                entry(2, DownloadState::Waiting),
                entry(3, DownloadState::Waiting),
                entry(4, DownloadState::Waiting),
            ],
            vec![2, 3, 4],
            Some(1),
        ),
        DownloadCommand::StopRange {
            gallery_ids: vec![1, 3],
        },
    );
    assert_eq!(plan.cancel_gallery_id, Some(1));
    assert_eq!(plan.start_gallery_id, Some(2));
    assert_eq!(plan.snapshot.waiting, vec![4]);
}

#[test]
fn activate_next_only_starts_an_idle_waiting_task() {
    let plan = reduce(
        snapshot(vec![entry(1, DownloadState::Waiting)], vec![1], None),
        DownloadCommand::ActivateNext,
    );
    assert_eq!(plan.start_gallery_id, Some(1));
    let unchanged = reduce(plan.snapshot.clone(), DownloadCommand::ActivateNext);
    assert_eq!(unchanged.snapshot, plan.snapshot);
}

#[test]
fn stopping_unknown_task_is_noop_and_stop_all_never_advances() {
    let initial = snapshot(
        vec![
            entry(1, DownloadState::Downloading),
            entry(2, DownloadState::Waiting),
        ],
        vec![2],
        Some(1),
    );
    let unchanged = reduce(initial.clone(), DownloadCommand::StopOne { gallery_id: 99 });
    assert_eq!(unchanged.snapshot, initial);
    let stopped = reduce(unchanged.snapshot, DownloadCommand::StopAll);
    assert_eq!(stopped.cancel_gallery_id, Some(1));
    assert!(stopped.snapshot.waiting.is_empty());
}

#[test]
fn finish_records_terminal_state_and_advances_next_task() {
    let initial = snapshot(
        vec![
            entry(1, DownloadState::Downloading),
            entry(2, DownloadState::Waiting),
        ],
        vec![2],
        Some(1),
    );
    let finished = reduce(
        initial.clone(),
        DownloadCommand::Finish {
            gallery_id: 1,
            failed: false,
        },
    );
    assert_eq!(finished.snapshot.entries[0].state, DownloadState::Finished);
    assert_eq!(finished.start_gallery_id, Some(2));
    let failed = reduce(
        initial,
        DownloadCommand::Finish {
            gallery_id: 1,
            failed: true,
        },
    );
    assert_eq!(failed.snapshot.entries[0].state, DownloadState::Failed);
}

#[test]
fn restart_normalization_only_resets_volatile_states() {
    let recovered = recover_download_queue(vec![
        entry(1, DownloadState::Waiting),
        entry(2, DownloadState::Downloading),
        entry(3, DownloadState::Finished),
        entry(4, DownloadState::Failed),
        entry(5, DownloadState::Invalid),
    ])
    .expect("persisted entries should recover");
    assert_eq!(recovered.snapshot.current, None);
    assert!(recovered.snapshot.waiting.is_empty());
    assert_eq!(
        recovered.snapshot.entries,
        vec![
            entry(1, DownloadState::None),
            entry(2, DownloadState::None),
            entry(3, DownloadState::Finished),
            entry(4, DownloadState::Failed),
            entry(5, DownloadState::Invalid)
        ]
    );
    assert_eq!(
        recover_download_queue(vec![entry(0, DownloadState::Waiting)])
            .expect_err("non-positive recovery id should fail"),
        DownloadQueueError::NonPositiveGalleryId(0)
    );
}

#[test]
fn serialized_command_and_plan_keep_stable_camel_case_fields() {
    let command = DownloadCommand::Finish {
        gallery_id: 7,
        failed: true,
    };
    assert_eq!(
        serde_json::to_value(&command).expect("serialize"),
        serde_json::json!({"kind":"finish","gid":7,"failed":true})
    );
    let plan = reduce(
        snapshot(vec![entry(7, DownloadState::None)], vec![], None),
        DownloadCommand::StartOne { gallery_id: 7 },
    );
    let json = serde_json::to_value(&plan).expect("serialize");
    assert_eq!(json["startGid"], 7);
    let request = DownloadQueueRequest {
        version: DOWNLOAD_QUEUE_API_VERSION,
        snapshot: snapshot(vec![entry(7, DownloadState::Downloading)], vec![], Some(7)),
        command: DownloadCommand::StartAll,
    };
    let request_bytes = serde_cbor::to_vec(&request).expect("request");
    let mut request_hex = String::new();
    for byte in &request_bytes {
        write!(&mut request_hex, "{byte:02x}").expect("string");
    }
    assert_eq!(
        request_hex,
        "a36776657273696f6e0168736e617073686f74a367656e747269657381a263676964076573746174656b646f776e6c6f6164696e676777616974696e67806763757272656e740767636f6d6d616e64a1646b696e64687374617274416c6c"
    );
    assert_eq!(
        serde_cbor::from_slice::<DownloadQueueRequest>(&request_bytes).expect("decode"),
        request
    );
    let recovery = DownloadQueueRecoveryRequest {
        version: DOWNLOAD_QUEUE_API_VERSION,
        entries: vec![entry(7, DownloadState::Downloading)],
    };
    let recovery_bytes = serde_cbor::to_vec(&recovery).expect("recovery");
    assert_eq!(
        serde_cbor::from_slice::<DownloadQueueRecoveryRequest>(&recovery_bytes).expect("decode"),
        recovery
    );
    let envelope = ApiEnvelope::ok(DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, plan);
    let envelope_bytes = serde_cbor::to_vec(&envelope).expect("envelope");
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<DownloadQueuePlan>>(&envelope_bytes).expect("decode"),
        envelope
    );
}

#[test]
fn invalid_snapshots_and_commands_report_typed_errors() {
    let cases = [
        (
            snapshot(vec![entry(0, DownloadState::None)], vec![], None),
            DownloadCommand::StartAll,
            DownloadQueueError::NonPositiveGalleryId(0),
        ),
        (
            snapshot(
                vec![entry(1, DownloadState::None), entry(1, DownloadState::None)],
                vec![],
                None,
            ),
            DownloadCommand::StartAll,
            DownloadQueueError::DuplicateEntry(1),
        ),
        (
            snapshot(vec![entry(1, DownloadState::None)], vec![], Some(2)),
            DownloadCommand::StartAll,
            DownloadQueueError::MissingCurrent(2),
        ),
        (
            snapshot(vec![entry(1, DownloadState::None)], vec![], Some(1)),
            DownloadCommand::StartAll,
            DownloadQueueError::CurrentState {
                gallery_id: 1,
                actual: DownloadState::None,
            },
        ),
        (
            snapshot(vec![entry(1, DownloadState::Downloading)], vec![], None),
            DownloadCommand::StartAll,
            DownloadQueueError::UnexpectedDownloading(1),
        ),
        (
            snapshot(vec![entry(1, DownloadState::Waiting)], vec![1, 1], None),
            DownloadCommand::StartAll,
            DownloadQueueError::DuplicateWaiting(1),
        ),
        (
            snapshot(vec![entry(1, DownloadState::None)], vec![2], None),
            DownloadCommand::StartAll,
            DownloadQueueError::MissingWaiting(2),
        ),
        (
            snapshot(vec![entry(1, DownloadState::None)], vec![1], None),
            DownloadCommand::StartAll,
            DownloadQueueError::WaitingState {
                gallery_id: 1,
                actual: DownloadState::None,
            },
        ),
        (
            snapshot(vec![entry(1, DownloadState::Waiting)], vec![], None),
            DownloadCommand::StartAll,
            DownloadQueueError::OrphanWaiting(1),
        ),
    ];
    for (snapshot, command, expected) in cases {
        assert_eq!(
            reduce_download_queue(snapshot, command).expect_err("snapshot should fail"),
            expected
        );
    }
    let valid = snapshot(vec![entry(1, DownloadState::None)], vec![], None);
    assert_eq!(
        reduce_download_queue(
            valid.clone(),
            DownloadCommand::StartRange {
                gallery_ids: vec![1, 1]
            }
        )
        .expect_err("duplicate"),
        DownloadQueueError::DuplicateCommandGalleryId(1)
    );
    assert_eq!(
        reduce_download_queue(
            valid,
            DownloadCommand::Finish {
                gallery_id: 1,
                failed: false
            }
        )
        .expect_err("stale finish"),
        DownloadQueueError::FinishNotCurrent {
            gallery_id: 1,
            current: None
        }
    );
}

#[test]
fn typed_queue_errors_have_stable_diagnostics() {
    let cases = [
        (
            DownloadQueueError::NonPositiveGalleryId(0),
            "gallery identifier must be positive: 0".to_owned(),
        ),
        (
            DownloadQueueError::DuplicateEntry(7),
            "duplicate download entry: 7".to_owned(),
        ),
        (
            DownloadQueueError::MissingCurrent(7),
            "active download entry is missing: 7".to_owned(),
        ),
        (
            DownloadQueueError::CurrentState {
                gallery_id: 7,
                actual: DownloadState::None,
            },
            "active download 7 has state None instead of Downloading".to_owned(),
        ),
        (
            DownloadQueueError::UnexpectedDownloading(7),
            "download 7 is active outside current".to_owned(),
        ),
        (
            DownloadQueueError::DuplicateWaiting(7),
            "duplicate waiting download: 7".to_owned(),
        ),
        (
            DownloadQueueError::MissingWaiting(7),
            "waiting download entry is missing: 7".to_owned(),
        ),
        (
            DownloadQueueError::WaitingState {
                gallery_id: 7,
                actual: DownloadState::Failed,
            },
            "waiting download 7 has state Failed instead of Waiting".to_owned(),
        ),
        (
            DownloadQueueError::OrphanWaiting(7),
            "waiting download is absent from FIFO: 7".to_owned(),
        ),
        (
            DownloadQueueError::DuplicateCommandGalleryId(7),
            "duplicate command gallery identifier: 7".to_owned(),
        ),
        (
            DownloadQueueError::MissingStartEntry(7),
            "download to start is missing: 7".to_owned(),
        ),
        (
            DownloadQueueError::FinishNotCurrent {
                gallery_id: 7,
                current: Some(8),
            },
            "finished download 7 does not match active download Some(8)".to_owned(),
        ),
    ];

    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}
