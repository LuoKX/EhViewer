use std::fmt::Write as _;

use serde_json::json;

use crate::contracts::ApiEnvelope;

use super::{
    MAX_READER_FETCH_ATTEMPT, READER_FETCH_API_VERSION, READER_FETCH_MODULE, ReaderFetchAction,
    ReaderFetchError, ReaderFetchErrorSource, ReaderFetchEvent, ReaderFetchImageFailure,
    ReaderFetchImageTarget, ReaderFetchPageSource, ReaderFetchPhase, ReaderFetchPlan,
    ReaderFetchRequest, ReaderFetchState, plan_reader_fetch,
};

#[test]
fn start_selects_cache_or_tokens_before_any_other_effect() {
    let normal = planned(initial(false, false, false), ReaderFetchEvent::Start);
    assert_eq!(normal.state.phase, ReaderFetchPhase::CheckingCache);
    assert_eq!(normal.actions, vec![ReaderFetchAction::CheckCache]);

    let forced = planned(initial(true, false, false), ReaderFetchEvent::Start);
    assert_eq!(forced.state.phase, ReaderFetchPhase::AcquiringTokens);
    assert_eq!(forced.actions, vec![ReaderFetchAction::AcquireTokens]);
}

#[test]
fn cache_and_token_outcomes_publish_exact_terminal_or_fetch_actions() {
    let checking = planned(initial(false, false, false), ReaderFetchEvent::Start).state;
    let hit = planned(checking.clone(), ReaderFetchEvent::CacheHit);
    assert_eq!(hit.state.phase, ReaderFetchPhase::Finished);
    assert_eq!(hit.actions, vec![ReaderFetchAction::SetFinished]);

    let acquiring = planned(checking, ReaderFetchEvent::CacheMiss).state;
    assert_eq!(
        planned(acquiring.clone(), ReaderFetchEvent::TokensMissing).actions,
        vec![ReaderFetchAction::SetFailed {
            error: ReaderFetchErrorSource::PTokenMessage,
        }]
    );

    let html = planned(
        acquiring.clone(),
        ReaderFetchEvent::TokensReady {
            show_key_available: false,
        },
    );
    assert_eq!(
        html.actions,
        vec![
            ReaderFetchAction::SetDownloading,
            ReaderFetchAction::FetchPage {
                source: ReaderFetchPageSource::Html,
            },
        ]
    );

    let api = planned(
        acquiring,
        ReaderFetchEvent::TokensReady {
            show_key_available: true,
        },
    );
    assert_eq!(api.state.page_source, Some(ReaderFetchPageSource::Api));
    assert_eq!(
        api.actions,
        vec![
            ReaderFetchAction::SetDownloading,
            ReaderFetchAction::FetchPage {
                source: ReaderFetchPageSource::Api,
            },
        ]
    );
}

#[test]
fn force_html_and_api_failure_choose_html_without_replacing_the_error() {
    let mut api = fetching(0, false, true, false, false, false);
    let retry = planned(api.clone(), ReaderFetchEvent::ApiRecoverableFailure);
    assert_eq!(retry.state.attempt, 1);
    assert!(retry.state.force_html);
    assert_eq!(
        retry.actions,
        vec![ReaderFetchAction::FetchPage {
            source: ReaderFetchPageSource::Html,
        }]
    );
    assert!(
        !retry
            .actions
            .contains(&ReaderFetchAction::CaptureCurrentError)
    );

    api.attempt = MAX_READER_FETCH_ATTEMPT;
    let exhausted = planned(api, ReaderFetchEvent::ApiRecoverableFailure);
    assert_eq!(exhausted.state.phase, ReaderFetchPhase::Failed);
    assert_eq!(
        exhausted.actions,
        vec![ReaderFetchAction::SetFailed {
            error: ReaderFetchErrorSource::Retained,
        }]
    );

    let forced_html = fetching(0, true, true, false, false, false);
    assert_eq!(forced_html.page_source, Some(ReaderFetchPageSource::Html));
    assert_eq!(
        plan_reader_fetch(forced_html, ReaderFetchEvent::ApiRecoverableFailure),
        Err(ReaderFetchError::InvalidTransition)
    );
}

#[test]
fn skip_hath_consumes_only_attempt_zero_and_forces_html() {
    let state = fetching(0, false, true, true, true, false);
    let skipped = planned(
        state,
        ReaderFetchEvent::PageReady {
            source: ReaderFetchPageSource::Api,
            origin_available: true,
            skip_key_available: true,
            show_key_available: true,
        },
    );
    assert_eq!(skipped.state.attempt, 1);
    assert!(skipped.state.force_html);
    assert_eq!(skipped.state.page_source, Some(ReaderFetchPageSource::Html));
    assert_eq!(
        skipped.actions,
        vec![ReaderFetchAction::FetchPage {
            source: ReaderFetchPageSource::Html,
        }]
    );

    let later = fetching(1, false, true, true, true, false);
    let not_skipped = planned(
        later,
        ReaderFetchEvent::PageReady {
            source: ReaderFetchPageSource::Api,
            origin_available: true,
            skip_key_available: false,
            show_key_available: true,
        },
    );
    assert_eq!(not_skipped.state.phase, ReaderFetchPhase::ResolvingOriginal);
}

#[test]
fn original_resolution_appends_skip_key_only_on_attempt_one() {
    for (attempt, skip_key_available, expected) in [
        (0, true, false),
        (1, false, false),
        (1, true, true),
        (2, true, false),
    ] {
        let page = fetching(attempt, false, false, false, true, false);
        let resolved = planned(
            page,
            ReaderFetchEvent::PageReady {
                source: ReaderFetchPageSource::Html,
                origin_available: true,
                skip_key_available,
                show_key_available: true,
            },
        );
        assert_eq!(resolved.state.phase, ReaderFetchPhase::ResolvingOriginal);
        assert_eq!(
            resolved.actions,
            vec![ReaderFetchAction::ResolveOriginal {
                append_skip_key: expected,
            }]
        );
    }

    let original = planned(
        resolving_original(1, true, false),
        ReaderFetchEvent::OriginalReady,
    );
    assert_eq!(original.state.phase, ReaderFetchPhase::DownloadingImage);
    assert_eq!(
        original.actions,
        vec![ReaderFetchAction::DownloadImage {
            target: ReaderFetchImageTarget::Original,
        }]
    );
}

#[test]
fn resized_selection_forces_all_later_page_fetches_to_html() {
    let page = fetching(0, false, true, false, false, false);
    let resized = planned(
        page,
        ReaderFetchEvent::PageReady {
            source: ReaderFetchPageSource::Api,
            origin_available: true,
            skip_key_available: true,
            show_key_available: true,
        },
    );
    assert!(resized.state.force_html);
    assert_eq!(
        resized.actions,
        vec![ReaderFetchAction::DownloadImage {
            target: ReaderFetchImageTarget::Resized,
        }]
    );

    let failed = planned(
        resized.state,
        ReaderFetchEvent::ImageFailed {
            failure: ReaderFetchImageFailure::Recoverable,
        },
    );
    assert_eq!(failed.state.page_source, Some(ReaderFetchPageSource::Html));
}

#[test]
fn outer_failure_preserves_fatal_capture_classify_and_invalidate_order() {
    let page = fetching(1, false, true, false, false, false);
    let fatal = planned(page, ReaderFetchEvent::OuterFailureRaised { fatal: true });
    assert_eq!(fatal.state.phase, ReaderFetchPhase::ClassifyingOuterFailure);
    assert_eq!(
        fatal.actions,
        vec![
            ReaderFetchAction::NotifyFatal,
            ReaderFetchAction::CaptureCurrentError,
        ]
    );
    let invalid = planned(
        fatal.state,
        ReaderFetchEvent::OuterFailureClassified { invalid_page: true },
    );
    assert_eq!(
        invalid.actions,
        vec![
            ReaderFetchAction::InvalidateToken,
            ReaderFetchAction::SetFailed {
                error: ReaderFetchErrorSource::Retained,
            },
        ]
    );

    let nonfatal = planned(
        resolving_original(0, true, false),
        ReaderFetchEvent::OuterFailureRaised { fatal: false },
    );
    assert_eq!(
        nonfatal.actions,
        vec![ReaderFetchAction::CaptureCurrentError]
    );
    assert_eq!(
        planned(
            nonfatal.state,
            ReaderFetchEvent::OuterFailureClassified {
                invalid_page: false,
            },
        )
        .actions,
        vec![ReaderFetchAction::SetFailed {
            error: ReaderFetchErrorSource::Retained,
        }]
    );
}

#[test]
fn image_success_recoverable_retry_and_exhaustion_preserve_action_order() {
    let image = downloading_image(0, false, true, true);
    let success = planned(image.clone(), ReaderFetchEvent::ImageSucceeded);
    assert_eq!(success.state.phase, ReaderFetchPhase::Finished);
    assert_eq!(success.actions, vec![ReaderFetchAction::SetFinished]);

    let retry = planned(
        image,
        ReaderFetchEvent::ImageFailed {
            failure: ReaderFetchImageFailure::Recoverable,
        },
    );
    assert_eq!(retry.state.attempt, 1);
    assert_eq!(retry.state.page_source, Some(ReaderFetchPageSource::Api));
    assert_eq!(
        retry.actions,
        vec![
            ReaderFetchAction::RemoveIntermediateFiles,
            ReaderFetchAction::CaptureCurrentError,
            ReaderFetchAction::FetchPage {
                source: ReaderFetchPageSource::Api,
            },
        ]
    );

    let exhausted = planned(
        downloading_image(MAX_READER_FETCH_ATTEMPT, false, true, true),
        ReaderFetchEvent::ImageFailed {
            failure: ReaderFetchImageFailure::Recoverable,
        },
    );
    assert_eq!(exhausted.state.phase, ReaderFetchPhase::Failed);
    assert_eq!(
        exhausted.actions,
        vec![
            ReaderFetchAction::RemoveIntermediateFiles,
            ReaderFetchAction::CaptureCurrentError,
            ReaderFetchAction::SetFailed {
                error: ReaderFetchErrorSource::Retained,
            },
        ]
    );
}

#[test]
fn file_not_found_and_cancellation_cleanup_before_outer_or_escape_actions() {
    let file_missing = planned(
        downloading_image(1, false, true, false),
        ReaderFetchEvent::ImageFailed {
            failure: ReaderFetchImageFailure::FileNotFound,
        },
    );
    assert_eq!(
        file_missing.actions,
        vec![
            ReaderFetchAction::RemoveIntermediateFiles,
            ReaderFetchAction::CaptureCurrentError,
        ]
    );
    assert_eq!(
        file_missing.state.phase,
        ReaderFetchPhase::ClassifyingOuterFailure
    );

    let cancellation = planned(
        downloading_image(1, false, true, false),
        ReaderFetchEvent::ImageFailed {
            failure: ReaderFetchImageFailure::Cancellation,
        },
    );
    assert_eq!(cancellation.state.phase, ReaderFetchPhase::Cancelled);
    assert_eq!(
        cancellation.actions,
        vec![
            ReaderFetchAction::RemoveIntermediateFiles,
            ReaderFetchAction::RethrowCancellation,
        ]
    );
    assert!(
        !cancellation
            .actions
            .contains(&ReaderFetchAction::NotifyFatal)
    );
    assert!(
        !cancellation
            .actions
            .contains(&ReaderFetchAction::InvalidateToken)
    );
}

#[test]
fn rejects_attempt_source_initial_cache_and_terminal_shape_drift() {
    let mut invalid_attempt = initial(false, false, false);
    invalid_attempt.attempt = MAX_READER_FETCH_ATTEMPT + 1;
    assert_eq!(
        plan_reader_fetch(invalid_attempt, ReaderFetchEvent::Start),
        Err(ReaderFetchError::AttemptOutOfBounds)
    );

    let mut invalid_initial = initial(false, false, false);
    invalid_initial.force_html = true;
    assert_eq!(
        plan_reader_fetch(invalid_initial, ReaderFetchEvent::Start),
        Err(ReaderFetchError::InvalidInitialState)
    );

    let mut invalid_source = initial(false, false, false);
    invalid_source.phase = ReaderFetchPhase::DownloadingImage;
    invalid_source.page_source = Some(ReaderFetchPageSource::Html);
    assert_eq!(
        plan_reader_fetch(invalid_source, ReaderFetchEvent::ImageSucceeded),
        Err(ReaderFetchError::InvalidPageSourceShape)
    );

    let mut missing_source = initial(false, false, false);
    missing_source.phase = ReaderFetchPhase::FetchingPage;
    assert_eq!(
        plan_reader_fetch(missing_source, ReaderFetchEvent::ApiRecoverableFailure),
        Err(ReaderFetchError::InvalidPageSourceShape)
    );

    let mut invalid_cache = initial(true, false, false);
    invalid_cache.phase = ReaderFetchPhase::CheckingCache;
    assert_eq!(
        plan_reader_fetch(invalid_cache, ReaderFetchEvent::CacheHit),
        Err(ReaderFetchError::InvalidCacheState)
    );

    assert_eq!(
        plan_reader_fetch(initial(false, true, false), ReaderFetchEvent::Start),
        Err(ReaderFetchError::InvalidSkipHathState)
    );

    for phase in [
        ReaderFetchPhase::Finished,
        ReaderFetchPhase::Failed,
        ReaderFetchPhase::Cancelled,
    ] {
        let terminal = ReaderFetchState {
            phase,
            ..initial(false, false, false)
        };
        assert_eq!(
            plan_reader_fetch(terminal, ReaderFetchEvent::Start),
            Err(ReaderFetchError::InvalidTransition)
        );
    }
}

#[test]
fn rejects_page_source_mismatch_and_events_in_wrong_phases() {
    let html = fetching(0, true, true, false, false, false);
    assert_eq!(
        plan_reader_fetch(
            html,
            ReaderFetchEvent::PageReady {
                source: ReaderFetchPageSource::Api,
                origin_available: false,
                skip_key_available: false,
                show_key_available: false,
            },
        ),
        Err(ReaderFetchError::PageSourceMismatch)
    );
    assert_eq!(
        plan_reader_fetch(
            resolving_original(0, true, false),
            ReaderFetchEvent::ImageSucceeded,
        ),
        Err(ReaderFetchError::InvalidTransition)
    );
}

#[test]
fn request_and_envelope_json_and_cbor_goldens_are_exact() {
    let request = ReaderFetchRequest {
        version: READER_FETCH_API_VERSION,
        state: fetching(1, false, true, false, true, false),
        event: ReaderFetchEvent::PageReady {
            source: ReaderFetchPageSource::Api,
            origin_available: true,
            skip_key_available: true,
            show_key_available: true,
        },
    };
    let result = planned(request.state.clone(), request.event);
    let envelope = ApiEnvelope::ok(
        READER_FETCH_API_VERSION,
        READER_FETCH_MODULE,
        result.clone(),
    );

    assert_eq!(
        serde_json::to_value(&request).expect("request JSON"),
        json!({
            "version": 1,
            "state": {
                "phase": "fetchingPage",
                "attempt": 1,
                "forceHtml": false,
                "force": false,
                "skipHath": false,
                "originalRequested": true,
                "showKeyAvailable": true,
                "pageSource": "api",
            },
            "event": {
                "kind": "pageReady",
                "source": "api",
                "originAvailable": true,
                "skipKeyAvailable": true,
                "showKeyAvailable": true,
            },
        })
    );
    assert_eq!(
        hex(&serde_cbor::to_vec(&request).expect("request CBOR")),
        REQUEST_CBOR_HEX
    );
    assert_eq!(
        hex(&serde_cbor::to_vec(&envelope).expect("response CBOR")),
        RESPONSE_CBOR_HEX
    );
    assert_eq!(
        serde_cbor::from_slice::<ReaderFetchRequest>(
            &serde_cbor::to_vec(&request).expect("request round-trip bytes")
        )
        .expect("request CBOR round trip"),
        request
    );
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<ReaderFetchPlan>>(
            &serde_cbor::to_vec(&envelope).expect("response round-trip bytes")
        )
        .expect("response CBOR round trip"),
        envelope
    );
}

#[test]
fn every_error_has_a_sanitized_stable_diagnostic() {
    let errors = [
        ReaderFetchError::AttemptOutOfBounds,
        ReaderFetchError::InvalidInitialState,
        ReaderFetchError::InvalidCacheState,
        ReaderFetchError::InvalidPageSourceShape,
        ReaderFetchError::InvalidSkipHathState,
        ReaderFetchError::InvalidTransition,
        ReaderFetchError::PageSourceMismatch,
    ];
    for error in errors {
        assert!(!error.to_string().is_empty());
        assert!(std::error::Error::source(&error).is_none());
    }
}

fn initial(force: bool, skip_hath: bool, original_requested: bool) -> ReaderFetchState {
    ReaderFetchState::initial(force, skip_hath, original_requested)
}

// Test fixtures name each independent V1 wire fact explicitly for readable cases.
#[allow(clippy::fn_params_excessive_bools)]
fn fetching(
    attempt: u8,
    force_html: bool,
    show_key_available: bool,
    skip_hath: bool,
    original_requested: bool,
    force: bool,
) -> ReaderFetchState {
    let source = if force_html || !show_key_available {
        ReaderFetchPageSource::Html
    } else {
        ReaderFetchPageSource::Api
    };
    ReaderFetchState {
        phase: ReaderFetchPhase::FetchingPage,
        attempt,
        force_html,
        force: force || skip_hath,
        skip_hath,
        original_requested,
        show_key_available,
        page_source: Some(source),
    }
}

fn resolving_original(attempt: u8, show_key_available: bool, force_html: bool) -> ReaderFetchState {
    ReaderFetchState {
        phase: ReaderFetchPhase::ResolvingOriginal,
        attempt,
        force_html,
        force: false,
        skip_hath: false,
        original_requested: true,
        show_key_available,
        page_source: None,
    }
}

fn downloading_image(
    attempt: u8,
    force_html: bool,
    show_key_available: bool,
    original_requested: bool,
) -> ReaderFetchState {
    ReaderFetchState {
        phase: ReaderFetchPhase::DownloadingImage,
        attempt,
        force_html,
        force: false,
        skip_hath: false,
        original_requested,
        show_key_available,
        page_source: None,
    }
}

fn planned(state: ReaderFetchState, event: ReaderFetchEvent) -> ReaderFetchPlan {
    plan_reader_fetch(state, event).expect("valid transition")
}

fn hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

const REQUEST_CBOR_HEX: &str = concat!(
    "a36776657273696f6e01657374617465a86570686173656c6665746368696e67506167656761",
    "7474656d70740169666f72636548746d6cf465666f726365f468736b697048617468f4716f72",
    "6967696e616c526571756573746564f57073686f774b6579417661696c61626c65f56a706167",
    "65536f7572636563617069656576656e74a5646b696e646970616765526561647966736f7572",
    "6365636170696f6f726967696e417661696c61626c65f570736b69704b6579417661696c6162",
    "6c65f57073686f774b6579417661696c61626c65f5",
);
const RESPONSE_CBOR_HEX: &str = concat!(
    "a46776657273696f6e01666d6f64756c656b7265616465724665746368677061796c6f6164",
    "a2657374617465a8657068617365717265736f6c76696e674f726967696e616c67617474656d",
    "70740169666f72636548746d6cf465666f726365f468736b697048617468f4716f726967696e",
    "616c526571756573746564f57073686f774b6579417661696c61626c65f56a70616765536f75",
    "726365f667616374696f6e7381a2646b696e646f7265736f6c76654f726967696e616c6d6170",
    "70656e64536b69704b6579f5656572726f72f6",
);
