//! Pure batching, rate-limit, and retry plans for Android gallery networking.
//!
//! This module owns deterministic policy only. Android retains HTTP transport,
//! response parsing, coroutine cancellation, concurrent wave execution, and delay
//! effects. One operation is planned before any request starts; no JNI call is
//! required per metadata batch or archive retry.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Gallery network-policy protocol version.
pub const API_VERSION: u8 = 1;

/// Stable module identifier used by the Android bridge.
pub const MODULE: &str = "galleryNetworkPolicy";

const METADATA_ITEMS_PER_REQUEST: i32 = 25;
const METADATA_REQUESTS_PER_WAVE: i32 = 5;
const METADATA_WAVE_DELAY_MILLIS: i64 = 5_000;
const ARCHIVE_RETRY_DELAY_MILLIS: i64 = 1_000;
const UNSUPPORTED_VERSION: &str = "unsupported gallery network policy protocol version";
const INVALID_REQUEST: &str = "invalid gallery network policy request shape";

/// Coarse gallery operation whose portable network policy is being planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryNetworkPolicyKind {
    /// Gallery Metadata `gdata` batching and wave rate limiting.
    Metadata,
    /// Gallery Archive download parsing and bounded null-result retry.
    ArchiveDownload,
}

/// Versioned input for one complete gallery network-policy operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryNetworkPolicyRequest {
    /// Requested protocol version.
    pub version: u8,
    /// Operation whose policy should be planned.
    pub kind: GalleryNetworkPolicyKind,
    /// Number of metadata items, present only for [`GalleryNetworkPolicyKind::Metadata`].
    pub item_count: Option<i32>,
    /// Whether an archive download uses H@H, present only for
    /// [`GalleryNetworkPolicyKind::ArchiveDownload`].
    pub is_hath: Option<bool>,
}

/// Complete policy response for one gallery networking operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryNetworkPolicyPlan {
    /// Planned operation kind.
    pub kind: GalleryNetworkPolicyKind,
    /// Metadata batching policy, present only for metadata operations.
    pub metadata: Option<GalleryMetadataPolicyPlan>,
    /// Archive attempt policy, present only for archive-download operations.
    pub archive_download: Option<GalleryArchiveDownloadPolicyPlan>,
}

/// Whole-list Gallery Metadata batching and wave-rate-limit plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryMetadataPolicyPlan {
    /// Item count supplied by Android for stale-plan validation.
    pub item_count: i32,
    /// Maximum gallery items carried by one `gdata` request.
    pub items_per_request: i32,
    /// Maximum requests executed concurrently in one wave.
    pub requests_per_wave: i32,
    /// Total request batches needed for `item_count`.
    pub request_count: i32,
    /// Total sequential waves needed for `request_count`.
    pub wave_count: i32,
    /// Delay before every wave after the first, in milliseconds.
    pub wave_delay_millis: i64,
}

/// One archive HTTP attempt and its delay relative to the previous null result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryArchiveAttemptPlan {
    /// Delay before this attempt, in milliseconds; the first attempt is always zero.
    pub delay_before_millis: i64,
}

/// Action taken when an archive response parser returns a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryArchiveParsedOutcome {
    /// Return the parsed direct-download URL.
    ReturnUrl,
    /// Discard the parsed value and preserve the H@H `null` result.
    ReturnNull,
}

/// Action taken after all planned archive attempts return `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryArchiveNullOutcome {
    /// Preserve the H@H `null` result.
    ReturnNull,
    /// Raise Android's established `Archive unavailable` failure.
    ArchiveUnavailable,
}

/// Whole-operation Gallery Archive attempt and terminal-outcome plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryArchiveDownloadPolicyPlan {
    /// Whether the selected archive option is H@H.
    pub is_hath: bool,
    /// Ordered attempts. Android retries only by advancing after a parsed `null`.
    pub attempts: Vec<GalleryArchiveAttemptPlan>,
    /// Outcome for the first non-null parsed URL.
    pub parsed_outcome: GalleryArchiveParsedOutcome,
    /// Outcome when every planned parse result is `null`.
    pub exhausted_null_outcome: GalleryArchiveNullOutcome,
}

/// Plans one whole Gallery Metadata or Gallery Archive networking operation.
///
/// # Errors
///
/// Returns one sanitized error when the operation kind and nullable request fields
/// disagree, or when a metadata item count is negative.
pub fn plan(
    request: &GalleryNetworkPolicyRequest,
) -> Result<GalleryNetworkPolicyPlan, &'static str> {
    match (request.kind, request.item_count, request.is_hath) {
        (GalleryNetworkPolicyKind::Metadata, Some(item_count), None) if item_count >= 0 => {
            let request_count = ceil_div_non_negative(item_count, METADATA_ITEMS_PER_REQUEST);
            let wave_count = ceil_div_non_negative(request_count, METADATA_REQUESTS_PER_WAVE);
            Ok(GalleryNetworkPolicyPlan {
                kind: request.kind,
                metadata: Some(GalleryMetadataPolicyPlan {
                    item_count,
                    items_per_request: METADATA_ITEMS_PER_REQUEST,
                    requests_per_wave: METADATA_REQUESTS_PER_WAVE,
                    request_count,
                    wave_count,
                    wave_delay_millis: METADATA_WAVE_DELAY_MILLIS,
                }),
                archive_download: None,
            })
        }
        (GalleryNetworkPolicyKind::ArchiveDownload, None, Some(is_hath)) => {
            let (attempts, parsed_outcome, exhausted_null_outcome) = if is_hath {
                (
                    vec![GalleryArchiveAttemptPlan {
                        delay_before_millis: 0,
                    }],
                    GalleryArchiveParsedOutcome::ReturnNull,
                    GalleryArchiveNullOutcome::ReturnNull,
                )
            } else {
                (
                    vec![
                        GalleryArchiveAttemptPlan {
                            delay_before_millis: 0,
                        },
                        GalleryArchiveAttemptPlan {
                            delay_before_millis: ARCHIVE_RETRY_DELAY_MILLIS,
                        },
                    ],
                    GalleryArchiveParsedOutcome::ReturnUrl,
                    GalleryArchiveNullOutcome::ArchiveUnavailable,
                )
            };
            Ok(GalleryNetworkPolicyPlan {
                kind: request.kind,
                metadata: None,
                archive_download: Some(GalleryArchiveDownloadPolicyPlan {
                    is_hath,
                    attempts,
                    parsed_outcome,
                    exhausted_null_outcome,
                }),
            })
        }
        _ => Err(INVALID_REQUEST),
    }
}

#[allow(
    clippy::manual_div_ceil,
    reason = "the pinned Rust toolchain still gates signed integer div_ceil"
)]
fn ceil_div_non_negative(value: i32, divisor: i32) -> i32 {
    value / divisor + i32::from(value % divisor != 0)
}

/// Builds the stable success/error envelope consumed by Android.
#[must_use]
pub fn build_envelope(
    request: &GalleryNetworkPolicyRequest,
) -> ApiEnvelope<GalleryNetworkPolicyPlan> {
    if request.version != API_VERSION {
        return ApiEnvelope::err(
            API_VERSION,
            MODULE,
            RustError::Error(UNSUPPORTED_VERSION.to_owned()),
        );
    }
    match plan(request) {
        Ok(policy) => ApiEnvelope::ok(API_VERSION, MODULE, policy),
        Err(error) => ApiEnvelope::err(API_VERSION, MODULE, RustError::Error(error.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_policy_preserves_batch_wave_and_delay_boundaries() {
        let cases = [
            (0, 0, 0),
            (1, 1, 1),
            (25, 1, 1),
            (26, 2, 1),
            (125, 5, 1),
            (126, 6, 2),
            (i32::MAX, 85_899_346, 17_179_870),
        ];

        for (item_count, request_count, wave_count) in cases {
            let planned = successful(&metadata_request(item_count));
            assert_eq!(planned.kind, GalleryNetworkPolicyKind::Metadata);
            assert_eq!(planned.archive_download, None);
            assert_eq!(
                planned.metadata,
                Some(GalleryMetadataPolicyPlan {
                    item_count,
                    items_per_request: 25,
                    requests_per_wave: 5,
                    request_count,
                    wave_count,
                    wave_delay_millis: 5_000,
                })
            );
        }
    }

    #[test]
    fn archive_policy_retries_only_direct_null_results_once() {
        let direct = successful(&archive_request(false));
        assert_eq!(direct.kind, GalleryNetworkPolicyKind::ArchiveDownload);
        assert_eq!(direct.metadata, None);
        assert_eq!(
            direct.archive_download,
            Some(GalleryArchiveDownloadPolicyPlan {
                is_hath: false,
                attempts: vec![
                    GalleryArchiveAttemptPlan {
                        delay_before_millis: 0,
                    },
                    GalleryArchiveAttemptPlan {
                        delay_before_millis: 1_000,
                    },
                ],
                parsed_outcome: GalleryArchiveParsedOutcome::ReturnUrl,
                exhausted_null_outcome: GalleryArchiveNullOutcome::ArchiveUnavailable,
            })
        );

        let hath = successful(&archive_request(true));
        assert_eq!(
            hath.archive_download,
            Some(GalleryArchiveDownloadPolicyPlan {
                is_hath: true,
                attempts: vec![GalleryArchiveAttemptPlan {
                    delay_before_millis: 0,
                }],
                parsed_outcome: GalleryArchiveParsedOutcome::ReturnNull,
                exhausted_null_outcome: GalleryArchiveNullOutcome::ReturnNull,
            })
        );
    }

    #[test]
    fn invalid_optional_shapes_and_negative_counts_return_one_sanitized_error() {
        let cases = [
            request(GalleryNetworkPolicyKind::Metadata, None, None),
            request(GalleryNetworkPolicyKind::Metadata, Some(-1), None),
            request(GalleryNetworkPolicyKind::Metadata, Some(1), Some(false)),
            request(GalleryNetworkPolicyKind::ArchiveDownload, None, None),
            request(
                GalleryNetworkPolicyKind::ArchiveDownload,
                Some(1),
                Some(false),
            ),
            request(GalleryNetworkPolicyKind::ArchiveDownload, Some(1), None),
        ];

        for request in cases {
            assert_eq!(plan(&request), Err(INVALID_REQUEST));
            assert_eq!(
                build_envelope(&request).error,
                Some(RustError::Error(INVALID_REQUEST.to_owned()))
            );
        }
    }

    #[test]
    fn envelopes_are_versioned_and_never_mix_payload_with_error() {
        let success = build_envelope(&metadata_request(126));
        assert_eq!(success.version, API_VERSION);
        assert_eq!(success.module, MODULE);
        assert!(success.payload.is_some());
        assert_eq!(success.error, None);

        let unsupported = GalleryNetworkPolicyRequest {
            version: API_VERSION + 1,
            ..archive_request(false)
        };
        let failure = build_envelope(&unsupported);
        assert_eq!(failure.version, API_VERSION);
        assert_eq!(failure.module, MODULE);
        assert_eq!(failure.payload, None);
        assert_eq!(
            failure.error,
            Some(RustError::Error(UNSUPPORTED_VERSION.to_owned()))
        );
    }

    #[test]
    fn json_and_cbor_round_trips_freeze_all_operation_shapes() {
        let requests = [
            metadata_request(126),
            archive_request(false),
            archive_request(true),
        ];
        for request in requests {
            let request_json =
                serde_json::to_string(&request).expect("policy request should encode as JSON");
            let decoded_request = serde_json::from_str(&request_json)
                .expect("policy request should decode from JSON");
            assert_eq!(request, decoded_request);

            let envelope = build_envelope(&request);
            let response_json =
                serde_json::to_string(&envelope).expect("policy envelope should encode as JSON");
            let decoded_response = serde_json::from_str(&response_json)
                .expect("policy envelope should decode from JSON");
            assert_eq!(envelope, decoded_response);

            let request_cbor =
                serde_cbor::to_vec(&request).expect("policy request should encode as CBOR");
            assert_eq!(
                serde_cbor::from_slice::<GalleryNetworkPolicyRequest>(&request_cbor)
                    .expect("policy request should decode from CBOR"),
                request
            );
            let response_cbor =
                serde_cbor::to_vec(&envelope).expect("policy envelope should encode as CBOR");
            assert_eq!(
                serde_cbor::from_slice::<ApiEnvelope<GalleryNetworkPolicyPlan>>(&response_cbor)
                    .expect("policy envelope should decode from CBOR"),
                envelope
            );
        }
    }

    fn metadata_request(item_count: i32) -> GalleryNetworkPolicyRequest {
        request(GalleryNetworkPolicyKind::Metadata, Some(item_count), None)
    }

    fn archive_request(is_hath: bool) -> GalleryNetworkPolicyRequest {
        request(
            GalleryNetworkPolicyKind::ArchiveDownload,
            None,
            Some(is_hath),
        )
    }

    fn request(
        kind: GalleryNetworkPolicyKind,
        item_count: Option<i32>,
        is_hath: Option<bool>,
    ) -> GalleryNetworkPolicyRequest {
        GalleryNetworkPolicyRequest {
            version: API_VERSION,
            kind,
            item_count,
            is_hath,
        }
    }

    fn successful(request: &GalleryNetworkPolicyRequest) -> GalleryNetworkPolicyPlan {
        plan(request).expect("valid policy request should produce a plan")
    }
}
