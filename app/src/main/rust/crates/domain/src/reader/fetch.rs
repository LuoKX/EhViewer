//! Deterministic reader-image fetch control flow.
//!
//! This module owns only the pure state transitions from cache selection through
//! token acquisition, page-source retries, original-image resolution, and the
//! terminal page result. Android retains every effect: cache and file access,
//! locks, throttling, network calls, URLs and tokens, localized errors, listeners,
//! and coroutine cancellation.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Protocol version of the reader-image fetch request and response.
pub const READER_FETCH_API_VERSION: u8 = 1;

/// Stable module identifier used by reader-image fetch response envelopes.
pub const READER_FETCH_MODULE: &str = "readerFetch";

/// Largest zero-based image attempt accepted by the reducer.
pub const MAX_READER_FETCH_ATTEMPT: u8 = 2;

/// Closed phase of one reader-image fetch operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderFetchPhase {
    /// No reducer event has been applied.
    Initial,
    /// Android must check its cache before any copy or file effect occurs.
    CheckingCache,
    /// Android must acquire the current and previous page tokens and throttle.
    AcquiringTokens,
    /// Android is fetching page metadata from the selected source.
    FetchingPage,
    /// Android is resolving the original image target.
    ResolvingOriginal,
    /// Android is downloading the selected original or resized image.
    DownloadingImage,
    /// Android has captured an outer failure and must classify its display value.
    ClassifyingOuterFailure,
    /// The page completed successfully.
    Finished,
    /// The page completed with a retained Android-owned display error.
    Failed,
    /// Image download cancellation was cleaned up and must be rethrown.
    Cancelled,
}

/// Closed page-metadata source selected for one attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderFetchPageSource {
    /// Fetch gallery page HTML.
    Html,
    /// Fetch gallery page metadata through the API.
    Api,
}

/// Closed image target selected after page metadata is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderFetchImageTarget {
    /// Resolve and download the original-resolution image.
    Original,
    /// Download the resized image URL from page metadata.
    Resized,
}

/// Android-classified image-download failure kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderFetchImageFailure {
    /// Retryable image failure, including an image-location `FatalException`.
    Recoverable,
    /// File lookup failure which must enter outer-failure classification.
    FileNotFound,
    /// Coroutine cancellation which must escape after intermediate cleanup.
    Cancellation,
}

/// Android-owned display-error source used when publishing a failed page state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderFetchErrorSource {
    /// Localized token-acquisition failure message retained by Android.
    PTokenMessage,
    /// Previously retained or just-captured Android display error.
    Retained,
}

/// Complete sanitized state for one deterministic reader-image fetch operation.
// These independent booleans are frozen V1 wire facts, not interchangeable binary states.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderFetchState {
    /// Current closed control-flow phase.
    pub phase: ReaderFetchPhase,
    /// Current zero-based attempt in the inclusive range `0..=2`.
    pub attempt: u8,
    /// Whether all later page metadata must use HTML.
    pub force_html: bool,
    /// Stable Android request input which bypasses the cache when true.
    pub force: bool,
    /// Stable Android request input which consumes attempt zero without downloading.
    pub skip_hath: bool,
    /// Stable Android request input selecting original images when available.
    pub original_requested: bool,
    /// Whether Android currently retains a usable API show key.
    pub show_key_available: bool,
    /// Selected source, present exactly while [`ReaderFetchPhase::FetchingPage`].
    pub page_source: Option<ReaderFetchPageSource>,
}

impl ReaderFetchState {
    /// Creates the only valid initial state for one Android request.
    #[must_use]
    pub const fn initial(force: bool, skip_hath: bool, original_requested: bool) -> Self {
        Self {
            phase: ReaderFetchPhase::Initial,
            attempt: 0,
            force_html: false,
            force,
            skip_hath,
            original_requested,
            show_key_available: false,
            page_source: None,
        }
    }
}

/// One closed Android observation applied to a [`ReaderFetchState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReaderFetchEvent {
    /// Starts one operation before any cache evaluation.
    Start,
    /// Reports an allowed cache hit.
    CacheHit,
    /// Reports a cache miss or a forced cache bypass.
    CacheMiss,
    /// Reports acquired tokens and the current shared show-key availability.
    TokensReady {
        /// Whether Android holds a usable show key after token acquisition.
        #[serde(rename = "showKeyAvailable")]
        show_key_available: bool,
    },
    /// Reports that the current page token could not be acquired.
    TokensMissing,
    /// Reports successful HTML/API page metadata without passing any source data.
    PageReady {
        /// Source which produced this result; it must equal the requested source.
        source: ReaderFetchPageSource,
        /// Whether Android retains an original-image location.
        #[serde(rename = "originAvailable")]
        origin_available: bool,
        /// Whether Android retains an H@H skip key.
        #[serde(rename = "skipKeyAvailable")]
        skip_key_available: bool,
        /// Whether Android retains a usable show key after this page result.
        #[serde(rename = "showKeyAvailable")]
        show_key_available: bool,
    },
    /// Reports a recoverable API failure which must not replace display error state.
    ApiRecoverableFailure,
    /// Reports a non-cancellation outer failure after Android classifies fatality.
    OuterFailureRaised {
        /// Whether Android must notify its fatal listener before capturing the error.
        fatal: bool,
    },
    /// Reports whether the captured outer display value is the exact invalid-page case.
    OuterFailureClassified {
        /// Whether Android must invalidate the current page token.
        #[serde(rename = "invalidPage")]
        invalid_page: bool,
    },
    /// Reports successful original-image URL resolution.
    OriginalReady,
    /// Reports successful image download and storage.
    ImageSucceeded,
    /// Reports one closed image-download failure kind.
    ImageFailed {
        /// Android-classified failure without exception text.
        failure: ReaderFetchImageFailure,
    },
}

/// One ordered Android effect emitted by the reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReaderFetchAction {
    /// Evaluate the cache shortcut after implementation selection.
    CheckCache,
    /// Acquire tokens and perform the Android-owned FIFO throttle.
    AcquireTokens,
    /// Publish the page as downloading.
    SetDownloading,
    /// Publish the page as successfully finished.
    SetFinished,
    /// Publish the page as failed with one Android-owned display source.
    SetFailed {
        /// Display-error source; no error text crosses this boundary.
        error: ReaderFetchErrorSource,
    },
    /// Fetch page metadata through one selected source.
    FetchPage {
        /// HTML or API source selected by the reducer.
        source: ReaderFetchPageSource,
    },
    /// Resolve the Android-owned original image location.
    ResolveOriginal {
        /// Whether Android must append its retained skip key before resolution.
        #[serde(rename = "appendSkipKey")]
        append_skip_key: bool,
    },
    /// Download one Android-owned image target.
    DownloadImage {
        /// Original or resized target selected by the reducer.
        target: ReaderFetchImageTarget,
    },
    /// Remove cache and temporary file intermediates before failure handling.
    RemoveIntermediateFiles,
    /// Capture the current Android exception's display value without crossing JNI.
    CaptureCurrentError,
    /// Notify Android's fatal listener.
    NotifyFatal,
    /// Remove the current page token after exact invalid-page classification.
    InvalidateToken,
    /// Rethrow Android coroutine cancellation after cleanup.
    RethrowCancellation,
}

/// Versioned request for one reducer transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderFetchRequest {
    /// Protocol version; callers must use [`READER_FETCH_API_VERSION`].
    pub version: u8,
    /// Complete reducer state before the event.
    pub state: ReaderFetchState,
    /// One closed Android observation.
    pub event: ReaderFetchEvent,
}

/// Deterministic next state and exact ordered Android effects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderFetchPlan {
    /// Complete reducer state after the event.
    pub state: ReaderFetchState,
    /// Effects which Android must apply once and in order.
    pub actions: Vec<ReaderFetchAction>,
}

/// Structural reason a reader-image fetch transition was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderFetchError {
    /// Attempt exceeded [`MAX_READER_FETCH_ATTEMPT`].
    AttemptOutOfBounds,
    /// Initial-only fields did not describe a fresh request.
    InvalidInitialState,
    /// A cache-check state was inconsistent with the stable force input.
    InvalidCacheState,
    /// A selected page source was absent or present in the wrong phase.
    InvalidPageSourceShape,
    /// The skip-H@H input was set without a forced request.
    InvalidSkipHathState,
    /// The event is not accepted in the current phase.
    InvalidTransition,
    /// A page result did not come from the source selected by the reducer.
    PageSourceMismatch,
}

impl fmt::Display for ReaderFetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AttemptOutOfBounds => "reader fetch attempt is outside the supported range",
            Self::InvalidInitialState => "reader fetch initial state is invalid",
            Self::InvalidCacheState => "reader fetch cache state is invalid",
            Self::InvalidPageSourceShape => "reader fetch page source shape is invalid",
            Self::InvalidSkipHathState => "reader fetch skip-H@H state is invalid",
            Self::InvalidTransition => "reader fetch event is invalid for the current phase",
            Self::PageSourceMismatch => {
                "reader fetch page result source does not match the request"
            }
        })
    }
}

impl std::error::Error for ReaderFetchError {}

/// Applies one event to a sanitized reader-image fetch state.
///
/// The returned actions preserve legacy ordering. The function performs no IO,
/// retains no platform data, and never receives a page index, gallery identifier,
/// URL, token, path, body, cookie, exception, or display string.
///
/// # Errors
///
/// Returns [`ReaderFetchError`] for an invalid state shape, an attempt outside
/// `0..=2`, a terminal or otherwise unsupported transition, or a page-source
/// mismatch.
// Keeping the frozen transition table in one exhaustive match makes ordering auditable.
#[allow(clippy::too_many_lines)]
pub fn plan_reader_fetch(
    mut state: ReaderFetchState,
    event: ReaderFetchEvent,
) -> Result<ReaderFetchPlan, ReaderFetchError> {
    validate_state(&state)?;

    match (state.phase, event) {
        (ReaderFetchPhase::Initial, ReaderFetchEvent::Start) => {
            if state.force {
                state.phase = ReaderFetchPhase::AcquiringTokens;
                Ok(plan(state, vec![ReaderFetchAction::AcquireTokens]))
            } else {
                state.phase = ReaderFetchPhase::CheckingCache;
                Ok(plan(state, vec![ReaderFetchAction::CheckCache]))
            }
        }
        (ReaderFetchPhase::CheckingCache, ReaderFetchEvent::CacheHit)
        | (ReaderFetchPhase::DownloadingImage, ReaderFetchEvent::ImageSucceeded) => {
            state.phase = ReaderFetchPhase::Finished;
            Ok(plan(state, vec![ReaderFetchAction::SetFinished]))
        }
        (ReaderFetchPhase::CheckingCache, ReaderFetchEvent::CacheMiss) => {
            state.phase = ReaderFetchPhase::AcquiringTokens;
            Ok(plan(state, vec![ReaderFetchAction::AcquireTokens]))
        }
        (ReaderFetchPhase::AcquiringTokens, ReaderFetchEvent::TokensMissing) => {
            state.phase = ReaderFetchPhase::Failed;
            Ok(plan(
                state,
                vec![ReaderFetchAction::SetFailed {
                    error: ReaderFetchErrorSource::PTokenMessage,
                }],
            ))
        }
        (
            ReaderFetchPhase::AcquiringTokens,
            ReaderFetchEvent::TokensReady { show_key_available },
        ) => {
            state.show_key_available = show_key_available;
            let source = selected_page_source(&state);
            state.phase = ReaderFetchPhase::FetchingPage;
            state.page_source = Some(source);
            Ok(plan(
                state,
                vec![
                    ReaderFetchAction::SetDownloading,
                    ReaderFetchAction::FetchPage { source },
                ],
            ))
        }
        (ReaderFetchPhase::FetchingPage, ReaderFetchEvent::ApiRecoverableFailure) => {
            if state.page_source != Some(ReaderFetchPageSource::Api) {
                return Err(ReaderFetchError::InvalidTransition);
            }
            state.force_html = true;
            Ok(retry_page_fetch(state, Vec::new()))
        }
        (
            ReaderFetchPhase::FetchingPage,
            ReaderFetchEvent::PageReady {
                source,
                origin_available,
                skip_key_available,
                show_key_available,
            },
        ) => {
            if state.page_source != Some(source) {
                return Err(ReaderFetchError::PageSourceMismatch);
            }
            state.show_key_available = show_key_available;
            state.page_source = None;

            if state.attempt == 0 && state.skip_hath {
                state.force_html = true;
                state.attempt = 1;
                state.phase = ReaderFetchPhase::FetchingPage;
                state.page_source = Some(ReaderFetchPageSource::Html);
                return Ok(plan(
                    state,
                    vec![ReaderFetchAction::FetchPage {
                        source: ReaderFetchPageSource::Html,
                    }],
                ));
            }

            if state.original_requested && origin_available {
                let append_skip_key = state.attempt == 1 && skip_key_available;
                state.phase = ReaderFetchPhase::ResolvingOriginal;
                Ok(plan(
                    state,
                    vec![ReaderFetchAction::ResolveOriginal { append_skip_key }],
                ))
            } else {
                state.force_html = true;
                state.phase = ReaderFetchPhase::DownloadingImage;
                Ok(plan(
                    state,
                    vec![ReaderFetchAction::DownloadImage {
                        target: ReaderFetchImageTarget::Resized,
                    }],
                ))
            }
        }
        (ReaderFetchPhase::ResolvingOriginal, ReaderFetchEvent::OriginalReady) => {
            state.phase = ReaderFetchPhase::DownloadingImage;
            Ok(plan(
                state,
                vec![ReaderFetchAction::DownloadImage {
                    target: ReaderFetchImageTarget::Original,
                }],
            ))
        }
        (
            ReaderFetchPhase::FetchingPage | ReaderFetchPhase::ResolvingOriginal,
            ReaderFetchEvent::OuterFailureRaised { fatal },
        ) => {
            state.phase = ReaderFetchPhase::ClassifyingOuterFailure;
            state.page_source = None;
            let mut actions = Vec::with_capacity(if fatal { 2 } else { 1 });
            if fatal {
                actions.push(ReaderFetchAction::NotifyFatal);
            }
            actions.push(ReaderFetchAction::CaptureCurrentError);
            Ok(plan(state, actions))
        }
        (
            ReaderFetchPhase::ClassifyingOuterFailure,
            ReaderFetchEvent::OuterFailureClassified { invalid_page },
        ) => {
            state.phase = ReaderFetchPhase::Failed;
            let mut actions = Vec::with_capacity(if invalid_page { 2 } else { 1 });
            if invalid_page {
                actions.push(ReaderFetchAction::InvalidateToken);
            }
            actions.push(ReaderFetchAction::SetFailed {
                error: ReaderFetchErrorSource::Retained,
            });
            Ok(plan(state, actions))
        }
        (ReaderFetchPhase::DownloadingImage, ReaderFetchEvent::ImageFailed { failure }) => {
            match failure {
                ReaderFetchImageFailure::Cancellation => {
                    state.phase = ReaderFetchPhase::Cancelled;
                    Ok(plan(
                        state,
                        vec![
                            ReaderFetchAction::RemoveIntermediateFiles,
                            ReaderFetchAction::RethrowCancellation,
                        ],
                    ))
                }
                ReaderFetchImageFailure::FileNotFound => {
                    state.phase = ReaderFetchPhase::ClassifyingOuterFailure;
                    Ok(plan(
                        state,
                        vec![
                            ReaderFetchAction::RemoveIntermediateFiles,
                            ReaderFetchAction::CaptureCurrentError,
                        ],
                    ))
                }
                ReaderFetchImageFailure::Recoverable => Ok(retry_page_fetch(
                    state,
                    vec![
                        ReaderFetchAction::RemoveIntermediateFiles,
                        ReaderFetchAction::CaptureCurrentError,
                    ],
                )),
            }
        }
        _ => Err(ReaderFetchError::InvalidTransition),
    }
}

fn validate_state(state: &ReaderFetchState) -> Result<(), ReaderFetchError> {
    if state.attempt > MAX_READER_FETCH_ATTEMPT {
        return Err(ReaderFetchError::AttemptOutOfBounds);
    }
    if state.skip_hath && !state.force {
        return Err(ReaderFetchError::InvalidSkipHathState);
    }
    if state.phase == ReaderFetchPhase::Initial
        && (state.attempt != 0
            || state.force_html
            || state.show_key_available
            || state.page_source.is_some())
    {
        return Err(ReaderFetchError::InvalidInitialState);
    }
    if state.phase == ReaderFetchPhase::CheckingCache && state.force {
        return Err(ReaderFetchError::InvalidCacheState);
    }
    let source_shape_is_valid =
        (state.phase == ReaderFetchPhase::FetchingPage) == state.page_source.is_some();
    if !source_shape_is_valid {
        return Err(ReaderFetchError::InvalidPageSourceShape);
    }
    Ok(())
}

fn selected_page_source(state: &ReaderFetchState) -> ReaderFetchPageSource {
    if state.force_html || !state.show_key_available {
        ReaderFetchPageSource::Html
    } else {
        ReaderFetchPageSource::Api
    }
}

fn retry_page_fetch(
    mut state: ReaderFetchState,
    mut actions: Vec<ReaderFetchAction>,
) -> ReaderFetchPlan {
    state.page_source = None;
    if state.attempt < MAX_READER_FETCH_ATTEMPT {
        state.attempt += 1;
        let source = selected_page_source(&state);
        state.phase = ReaderFetchPhase::FetchingPage;
        state.page_source = Some(source);
        actions.push(ReaderFetchAction::FetchPage { source });
    } else {
        state.phase = ReaderFetchPhase::Failed;
        actions.push(ReaderFetchAction::SetFailed {
            error: ReaderFetchErrorSource::Retained,
        });
    }
    plan(state, actions)
}

fn plan(state: ReaderFetchState, actions: Vec<ReaderFetchAction>) -> ReaderFetchPlan {
    ReaderFetchPlan { state, actions }
}

#[cfg(test)]
mod tests;
