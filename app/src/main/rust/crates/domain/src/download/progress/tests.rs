use std::fmt::Write as _;

use crate::contracts::ApiEnvelope;

use super::*;

const REQUEST_CBOR_HEX: &str = concat!(
    "a36776657273696f6e0168736e617073686f74a7657374617465646e6f6e65737370656564",
    "42797465735065725365636f6e64077572656d61696e696e674d696c6c697365636f6e6473",
    "096866696e6973686564016a646f776e6c6f616465640265746f74616c03666c6567616379",
    "0467636f6d6d616e64a4646b696e646d73657450616765526573756c746866696e69736865",
    "64036a646f776e6c6f616465640465746f74616c0a"
);
const ESTIMATE_REQUEST_CBOR_HEX: &str = concat!(
    "a36776657273696f6e0168736e617073686f74a76573746174656b646f776e6c6f6164696e",
    "6773737065656442797465735065725365636f6e64207572656d61696e696e674d696c6c6973",
    "65636f6e6473206866696e6973686564006a646f776e6c6f616465640065746f74616c0a666c",
    "65676163792067636f6d6d616e64a3646b696e6468657374696d617465737370656564427974",
    "65735065725365636f6e64192000697472616e736665727381a26d636f6e74656e744c656e67",
    "7468206c726563656976656453697a65191000"
);
const RESPONSE_CBOR_HEX: &str = concat!(
    "a46776657273696f6e01666d6f64756c6570646f776e6c6f616450726f6772657373677061",
    "796c6f6164a168736e617073686f74a7657374617465646e6f6e6573737065656442797465",
    "735065725365636f6e64077572656d61696e696e674d696c6c697365636f6e647309686669",
    "6e6973686564036a646f776e6c6f616465640465746f74616c0a666c656761637904656572",
    "726f72f6"
);

fn snapshot() -> DownloadProgressSnapshot {
    DownloadProgressSnapshot {
        state: DownloadState::None,
        speed_bytes_per_second: 7,
        remaining_milliseconds: 9,
        finished: 1,
        downloaded: 2,
        total: 3,
        legacy: 4,
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn from_hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).expect("hex fixture should be ASCII");
            u8::from_str_radix(pair, 16).expect("hex fixture should be valid")
        })
        .collect()
}

#[test]
fn reset_reinitializes_every_runtime_counter() {
    let reset = plan_download_progress(snapshot(), DownloadProgressCommand::Reset).snapshot;

    assert_eq!(reset.state, DownloadState::Downloading);
    assert_eq!(reset.speed_bytes_per_second, -1);
    assert_eq!(reset.remaining_milliseconds, -1);
    assert_eq!(reset.finished, 0);
    assert_eq!(reset.downloaded, 0);
    assert_eq!(reset.total, -1);
    assert_eq!(reset.legacy, -1);
}

#[test]
fn set_pages_changes_only_the_total() {
    let before = snapshot();
    let actual = plan_download_progress(
        before.clone(),
        DownloadProgressCommand::SetPages { total: 8 },
    )
    .snapshot;

    assert_eq!(actual, DownloadProgressSnapshot { total: 8, ..before });
}

#[test]
fn set_page_result_changes_only_page_counters() {
    let before = snapshot();
    let actual = plan_download_progress(
        before.clone(),
        DownloadProgressCommand::SetPageResult {
            finished: 4,
            downloaded: 5,
            total: 8,
        },
    )
    .snapshot;

    assert_eq!(
        actual,
        DownloadProgressSnapshot {
            finished: 4,
            downloaded: 5,
            total: 8,
            ..before
        }
    );
}

#[test]
fn estimate_covers_unknown_stalled_empty_and_nontrivial_eta() {
    let base = DownloadProgressSnapshot {
        total: 10,
        downloaded: 2,
        remaining_milliseconds: 9,
        ..snapshot()
    };
    let unknown = plan_download_progress(
        DownloadProgressSnapshot {
            total: 0,
            ..base.clone()
        },
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second: 250,
            transfers: Vec::new(),
        },
    )
    .snapshot;
    assert_eq!(unknown.remaining_milliseconds, -1);

    let stalled = plan_download_progress(
        base.clone(),
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second: 0,
            transfers: Vec::new(),
        },
    )
    .snapshot;
    assert_eq!(
        stalled.remaining_milliseconds,
        300_i64 * 24 * 60 * 60 * 1000
    );

    let empty = plan_download_progress(
        base.clone(),
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second: 250,
            transfers: Vec::new(),
        },
    )
    .snapshot;
    assert_eq!(empty.remaining_milliseconds, 9);

    let estimated = plan_download_progress(
        base,
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second: 250,
            transfers: vec![
                ActiveDownloadTransfer {
                    content_length: 1_000,
                    received_size: 400,
                },
                ActiveDownloadTransfer {
                    content_length: 500,
                    received_size: 100,
                },
            ],
        },
    )
    .snapshot;
    assert_eq!(estimated.speed_bytes_per_second, 250);
    assert_eq!(estimated.remaining_milliseconds, 22_000);
}

#[test]
fn estimate_preserves_jvm_wrapping_for_overflow_and_minimum_division() {
    let actual = plan_download_progress(
        DownloadProgressSnapshot {
            total: 1,
            downloaded: 0,
            ..snapshot()
        },
        DownloadProgressCommand::Estimate {
            speed_bytes_per_second: -1,
            transfers: vec![ActiveDownloadTransfer {
                content_length: i64::MIN,
                received_size: 0,
            }],
        },
    )
    .snapshot;

    assert_eq!(wrapping_div(i64::MIN, -1), i64::MIN);
    assert_eq!(actual.remaining_milliseconds, 0);
}

#[test]
fn finish_sets_success_failure_and_wrapped_legacy_counter() {
    let success = plan_download_progress(
        snapshot(),
        DownloadProgressCommand::Finish {
            finished: i32::MIN,
            downloaded: 1,
            total: i32::MAX,
            failed: false,
        },
    )
    .snapshot;
    assert_eq!(success.state, DownloadState::Finished);
    assert_eq!(success.finished, i32::MIN);
    assert_eq!(success.downloaded, 1);
    assert_eq!(success.total, i32::MAX);
    assert_eq!(success.legacy, i32::MAX.wrapping_sub(i32::MIN));

    let failure = plan_download_progress(
        snapshot(),
        DownloadProgressCommand::Finish {
            finished: 3,
            downloaded: 4,
            total: 10,
            failed: true,
        },
    )
    .snapshot;
    assert_eq!(failure.state, DownloadState::Failed);
    assert_eq!(failure.legacy, 7);
}

#[test]
fn request_and_response_json_and_cbor_goldens_are_exact() {
    let request = DownloadProgressRequest {
        version: DOWNLOAD_PROGRESS_API_VERSION,
        snapshot: snapshot(),
        command: DownloadProgressCommand::SetPageResult {
            finished: 3,
            downloaded: 4,
            total: 10,
        },
    };
    let response = ApiEnvelope::ok(
        DOWNLOAD_PROGRESS_API_VERSION,
        DOWNLOAD_PROGRESS_MODULE,
        plan_download_progress(request.snapshot.clone(), request.command.clone()),
    );

    let request_json = serde_json::to_string(&request).expect("request JSON");
    let response_json = serde_json::to_string(&response).expect("response JSON");
    assert_eq!(
        request_json,
        r#"{"version":1,"snapshot":{"state":"none","speedBytesPerSecond":7,"remainingMilliseconds":9,"finished":1,"downloaded":2,"total":3,"legacy":4},"command":{"kind":"setPageResult","finished":3,"downloaded":4,"total":10}}"#
    );
    assert_eq!(
        response_json,
        r#"{"version":1,"module":"downloadProgress","payload":{"snapshot":{"state":"none","speedBytesPerSecond":7,"remainingMilliseconds":9,"finished":3,"downloaded":4,"total":10,"legacy":4}},"error":null}"#
    );

    let request_bytes = serde_cbor::to_vec(&request).expect("request CBOR");
    let response_bytes = serde_cbor::to_vec(&response).expect("response CBOR");
    assert_eq!(to_hex(&request_bytes), REQUEST_CBOR_HEX);
    assert_eq!(to_hex(&response_bytes), RESPONSE_CBOR_HEX);
    assert_eq!(
        serde_json::from_str::<DownloadProgressRequest>(&request_json)
            .expect("request JSON round trip"),
        request
    );
    assert_eq!(
        serde_cbor::from_slice::<DownloadProgressRequest>(&request_bytes)
            .expect("request CBOR round trip"),
        request
    );
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<DownloadProgressPlan>>(&response_bytes)
            .expect("response CBOR round trip"),
        response
    );
}

#[test]
fn kotlin_estimate_cbor_uses_camel_case_variant_fields() {
    let request = DownloadProgressRequest {
        version: DOWNLOAD_PROGRESS_API_VERSION,
        snapshot: DownloadProgressSnapshot {
            state: DownloadState::Downloading,
            speed_bytes_per_second: -1,
            remaining_milliseconds: -1,
            finished: 0,
            downloaded: 0,
            total: 10,
            legacy: -1,
        },
        command: DownloadProgressCommand::Estimate {
            speed_bytes_per_second: 8_192,
            transfers: vec![ActiveDownloadTransfer {
                content_length: -1,
                received_size: 4_096,
            }],
        },
    };

    let encoded = serde_cbor::to_vec(&request).expect("estimate request CBOR");
    assert_eq!(to_hex(&encoded), ESTIMATE_REQUEST_CBOR_HEX);
    assert_eq!(
        serde_cbor::from_slice::<DownloadProgressRequest>(&from_hex(ESTIMATE_REQUEST_CBOR_HEX))
            .expect("Kotlin estimate request should decode"),
        request
    );
}

#[test]
fn command_decode_rejects_missing_fields_and_unknown_kinds_but_ignores_future_fields() {
    assert!(serde_json::from_str::<DownloadProgressCommand>(r#"{"kind":"setPages"}"#).is_err());
    assert!(serde_json::from_str::<DownloadProgressCommand>(r#"{"kind":"unknown"}"#).is_err());
    assert_eq!(
        serde_json::from_str::<DownloadProgressCommand>(
            r#"{"kind":"setPages","total":4,"futureField":9}"#
        )
        .expect("unknown fields remain forward compatible"),
        DownloadProgressCommand::SetPages { total: 4 }
    );
}
