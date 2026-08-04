//! Pure reader-page scheduling and image-fetch decisions.
//!
//! Android provides the current page states, previous visible page, cache hit,
//! and one reader command. This module validates that snapshot and emits ordered
//! actions which preserve the legacy `PageLoader` behavior. [`fetch`] separately
//! owns the event-driven image-fetch control flow. Neither path owns image objects,
//! flows, locks, network requests, URLs, tokens, archive handles, or cache storage.

pub mod fetch;

use std::fmt;

use serde::{Deserialize, Serialize};

/// Protocol version of the reader-scheduling request and response.
pub const READER_SCHEDULE_API_VERSION: u8 = 1;

/// Stable module identifier used by the reader-scheduling response envelope.
pub const READER_SCHEDULE_MODULE: &str = "readerSchedule";

/// Maximum page count accepted from the Android reader.
pub const MAX_READER_PAGE_COUNT: usize = 100_000;

/// Maximum prefetch distance accepted from Android preferences.
pub const MAX_READER_PREFETCH_PAGE_COUNT: u32 = 1_024;

/// Platform-neutral status of one Android-owned reader page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReaderPageState {
    /// No request is active and the page may be prefetched.
    Queued,
    /// The page source is loading or decoding.
    Loading,
    /// The decoded page is hidden by the Android content filter.
    Blocked,
    /// The decoded page is ready for display.
    Ready,
    /// The previous request failed and the page may be prefetched again.
    Error,
}

/// Complete platform-independent input needed for one scheduling decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderScheduleSnapshot {
    /// Page states in stable zero-based index order.
    pub page_states: Vec<ReaderPageState>,
    /// Most recently requested index, or `None` before the first request.
    pub previous_index: Option<u32>,
}

/// One reader command applied to a [`ReaderScheduleSnapshot`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReaderScheduleCommand {
    /// Displays or fetches one page and schedules directional prefetching.
    Request {
        /// Zero-based page selected by Android.
        index: u32,
        /// Whether Android holds a decoded image for this page in its memory cache.
        cached: bool,
        /// Number of adjacent pages considered for directional prefetching.
        #[serde(rename = "prefetchPageCount")]
        prefetch_page_count: u32,
    },
    /// Clears one failed/cached page and requests it with explicit retry options.
    Retry {
        /// Zero-based page selected by Android.
        index: u32,
        /// Whether the retry must request the original-resolution image.
        #[serde(rename = "originalImage")]
        original_image: bool,
    },
    /// Clears every cached image and resets every page state.
    Restart,
}

/// Versioned request sent from Android to the Rust reader scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderScheduleRequest {
    /// Protocol version; callers must use [`READER_SCHEDULE_API_VERSION`].
    pub version: u8,
    /// Complete page-state and navigation snapshot.
    pub snapshot: ReaderScheduleSnapshot,
    /// Atomic scheduling command to plan.
    pub command: ReaderScheduleCommand,
}

/// One ordered platform action produced by the reader scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReaderScheduleAction {
    /// Records the current request index for the next direction decision.
    SetPrevious {
        /// Zero-based page index.
        index: u32,
    },
    /// Resets one Android page status to queued.
    ResetOne {
        /// Zero-based page index.
        index: u32,
    },
    /// Displays the decoded image leased from Android's memory cache.
    UseCached {
        /// Zero-based page index.
        index: u32,
    },
    /// Resets every Android page status to queued.
    ResetAll,
    /// Removes one decoded image from Android's memory cache.
    RemoveCache {
        /// Zero-based page index.
        index: u32,
    },
    /// Evicts every decoded image from Android's memory cache.
    ClearCache,
    /// Starts one Android-owned page source request.
    Fetch {
        /// Zero-based page index.
        index: u32,
        /// Whether the platform request must bypass a previous failure/cache entry.
        force: bool,
        /// Whether the platform request must use the original-resolution image.
        #[serde(rename = "originalImage")]
        original_image: bool,
    },
    /// Asks Android to prefetch eligible page sources within legacy request bounds.
    Prefetch {
        /// Eligible page indices in navigation order.
        pages: Vec<u32>,
        /// Inclusive lower bound forwarded to the Android prefetch adapter.
        #[serde(rename = "boundsStart")]
        bounds_start: i32,
        /// Inclusive upper bound forwarded to the Android prefetch adapter.
        #[serde(rename = "boundsEnd")]
        bounds_end: i32,
    },
}

/// Deterministic ordered effects for one reader scheduling command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderSchedulePlan {
    /// Actions which Android must apply exactly once and in this order.
    pub actions: Vec<ReaderScheduleAction>,
}

/// Structural reason that a reader snapshot or command cannot be planned safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderScheduleError {
    /// A reader snapshot contained no pages.
    EmptyPages,
    /// A reader snapshot exceeded the supported page-count bound.
    TooManyPages {
        /// Page count supplied by Android.
        actual: usize,
        /// Maximum accepted page count.
        maximum: usize,
    },
    /// The previous-page index was outside the snapshot.
    PreviousIndexOutOfBounds {
        /// Invalid previous-page index.
        index: u32,
        /// Number of pages in the snapshot.
        page_count: usize,
    },
    /// The command page index was outside the snapshot.
    PageIndexOutOfBounds {
        /// Invalid command page index.
        index: u32,
        /// Number of pages in the snapshot.
        page_count: usize,
    },
    /// The requested prefetch distance exceeded the defensive wire limit.
    PrefetchPageCountTooLarge {
        /// Prefetch distance supplied by Android.
        actual: u32,
        /// Maximum accepted prefetch distance.
        maximum: u32,
    },
}

impl fmt::Display for ReaderScheduleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPages => {
                formatter.write_str("reader snapshot must contain at least one page")
            }
            Self::TooManyPages { actual, maximum } => write!(
                formatter,
                "reader snapshot contains {actual} pages, exceeding the limit of {maximum}"
            ),
            Self::PreviousIndexOutOfBounds { index, page_count } => write!(
                formatter,
                "previous reader index {index} is outside {page_count} pages"
            ),
            Self::PageIndexOutOfBounds { index, page_count } => write!(
                formatter,
                "reader command index {index} is outside {page_count} pages"
            ),
            Self::PrefetchPageCountTooLarge { actual, maximum } => write!(
                formatter,
                "reader prefetch count {actual} exceeds the limit of {maximum}"
            ),
        }
    }
}

impl std::error::Error for ReaderScheduleError {}

/// Validates and plans one reader command without performing platform effects.
///
/// Request plans always record the new previous index before displaying/fetching
/// the page, then emit one prefetch action even when its page list is empty.
/// Retry and restart deliberately preserve the previous index. The returned action
/// order matches the existing Android `PageLoader` and must be applied exactly once.
///
/// # Errors
///
/// Returns [`ReaderScheduleError`] when the page list is empty or too large, when
/// an index is outside the snapshot, or when the prefetch distance exceeds the
/// defensive protocol limit.
pub fn plan_reader_schedule(
    snapshot: &ReaderScheduleSnapshot,
    command: ReaderScheduleCommand,
) -> Result<ReaderSchedulePlan, ReaderScheduleError> {
    validate_snapshot(snapshot)?;

    match command {
        ReaderScheduleCommand::Request {
            index,
            cached,
            prefetch_page_count,
        } => plan_request(snapshot, index, cached, prefetch_page_count),
        ReaderScheduleCommand::Retry {
            index,
            original_image,
        } => {
            validate_page_index(index, snapshot.page_states.len())?;
            Ok(ReaderSchedulePlan {
                actions: vec![
                    ReaderScheduleAction::ResetOne { index },
                    ReaderScheduleAction::RemoveCache { index },
                    ReaderScheduleAction::Fetch {
                        index,
                        force: true,
                        original_image,
                    },
                ],
            })
        }
        ReaderScheduleCommand::Restart => Ok(ReaderSchedulePlan {
            actions: vec![
                ReaderScheduleAction::ClearCache,
                ReaderScheduleAction::ResetAll,
            ],
        }),
    }
}

fn validate_snapshot(snapshot: &ReaderScheduleSnapshot) -> Result<(), ReaderScheduleError> {
    let page_count = snapshot.page_states.len();
    if page_count == 0 {
        return Err(ReaderScheduleError::EmptyPages);
    }
    if page_count > MAX_READER_PAGE_COUNT {
        return Err(ReaderScheduleError::TooManyPages {
            actual: page_count,
            maximum: MAX_READER_PAGE_COUNT,
        });
    }
    if let Some(index) = snapshot.previous_index {
        validate_page_index(index, page_count)
            .map_err(|_| ReaderScheduleError::PreviousIndexOutOfBounds { index, page_count })?;
    }
    Ok(())
}

fn validate_page_index(index: u32, page_count: usize) -> Result<(), ReaderScheduleError> {
    let index_in_bounds = usize::try_from(index).is_ok_and(|value| value < page_count);
    if index_in_bounds {
        Ok(())
    } else {
        Err(ReaderScheduleError::PageIndexOutOfBounds { index, page_count })
    }
}

fn plan_request(
    snapshot: &ReaderScheduleSnapshot,
    index: u32,
    cached: bool,
    prefetch_page_count: u32,
) -> Result<ReaderSchedulePlan, ReaderScheduleError> {
    validate_page_index(index, snapshot.page_states.len())?;
    if prefetch_page_count > MAX_READER_PREFETCH_PAGE_COUNT {
        return Err(ReaderScheduleError::PrefetchPageCountTooLarge {
            actual: prefetch_page_count,
            maximum: MAX_READER_PREFETCH_PAGE_COUNT,
        });
    }

    let forward = snapshot
        .previous_index
        .is_none_or(|previous_index| index >= previous_index);
    let (pages, bounds_start, bounds_end) =
        prefetch_plan(snapshot, index, prefetch_page_count, forward);
    let mut actions = Vec::with_capacity(if cached { 3 } else { 4 });
    actions.push(ReaderScheduleAction::SetPrevious { index });
    if cached {
        actions.push(ReaderScheduleAction::UseCached { index });
    } else {
        actions.push(ReaderScheduleAction::ResetOne { index });
        actions.push(ReaderScheduleAction::Fetch {
            index,
            force: false,
            original_image: false,
        });
    }
    actions.push(ReaderScheduleAction::Prefetch {
        pages,
        bounds_start,
        bounds_end,
    });
    Ok(ReaderSchedulePlan { actions })
}

fn prefetch_plan(
    snapshot: &ReaderScheduleSnapshot,
    index: u32,
    prefetch_page_count: u32,
    forward: bool,
) -> (Vec<u32>, i32, i32) {
    let index = i32::try_from(index).expect("validated reader index fits i32");
    let prefetch = i32::try_from(prefetch_page_count).expect("validated prefetch count fits i32");
    let last_page =
        i32::try_from(snapshot.page_states.len() - 1).expect("validated page count fits i32");

    if forward {
        let first = index + 1;
        let last = (index + prefetch).min(last_page);
        let pages = if first <= last {
            (first..=last)
                .filter_map(|candidate| prefetch_candidate(snapshot, candidate))
                .collect()
        } else {
            Vec::new()
        };
        (pages, first - 5, last + 5)
    } else {
        let first = index - 1;
        let last = (index - prefetch).max(0);
        let pages = if first >= last {
            (last..=first)
                .rev()
                .filter_map(|candidate| prefetch_candidate(snapshot, candidate))
                .collect()
        } else {
            Vec::new()
        };
        (pages, last - 5, first + 5)
    }
}

fn prefetch_candidate(snapshot: &ReaderScheduleSnapshot, candidate: i32) -> Option<u32> {
    let candidate = u32::try_from(candidate).expect("prefetch candidate is non-negative");
    let offset = usize::try_from(candidate).expect("prefetch candidate fits usize");
    matches!(
        snapshot.page_states[offset],
        ReaderPageState::Queued | ReaderPageState::Error
    )
    .then_some(candidate)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use serde_json::json;

    use crate::contracts::ApiEnvelope;

    use super::{
        MAX_READER_PAGE_COUNT, MAX_READER_PREFETCH_PAGE_COUNT, READER_SCHEDULE_API_VERSION,
        READER_SCHEDULE_MODULE, ReaderPageState, ReaderScheduleAction, ReaderScheduleCommand,
        ReaderScheduleError, ReaderSchedulePlan, ReaderScheduleRequest, ReaderScheduleSnapshot,
        plan_reader_schedule,
    };

    #[test]
    fn forward_cache_miss_preserves_action_order_and_filters_prefetch_states() {
        let snapshot = snapshot(
            &[
                ReaderPageState::Ready,
                ReaderPageState::Blocked,
                ReaderPageState::Loading,
                ReaderPageState::Queued,
                ReaderPageState::Error,
                ReaderPageState::Ready,
                ReaderPageState::Queued,
            ],
            Some(1),
        );

        let plan = plan_reader_schedule(
            &snapshot,
            ReaderScheduleCommand::Request {
                index: 2,
                cached: false,
                prefetch_page_count: 3,
            },
        )
        .expect("forward request should plan");

        assert_eq!(
            plan.actions,
            vec![
                ReaderScheduleAction::SetPrevious { index: 2 },
                ReaderScheduleAction::ResetOne { index: 2 },
                ReaderScheduleAction::Fetch {
                    index: 2,
                    force: false,
                    original_image: false,
                },
                ReaderScheduleAction::Prefetch {
                    pages: vec![3, 4],
                    bounds_start: -2,
                    bounds_end: 10,
                },
            ]
        );
    }

    #[test]
    fn first_request_with_cache_hit_uses_cached_image_before_prefetch() {
        let snapshot = snapshot(
            &[
                ReaderPageState::Queued,
                ReaderPageState::Ready,
                ReaderPageState::Loading,
            ],
            None,
        );

        let plan = plan_reader_schedule(
            &snapshot,
            ReaderScheduleCommand::Request {
                index: 1,
                cached: true,
                prefetch_page_count: 1,
            },
        )
        .expect("cached request should plan");

        assert_eq!(
            plan.actions,
            vec![
                ReaderScheduleAction::SetPrevious { index: 1 },
                ReaderScheduleAction::UseCached { index: 1 },
                ReaderScheduleAction::Prefetch {
                    pages: Vec::new(),
                    bounds_start: -3,
                    bounds_end: 7,
                },
            ]
        );
    }

    #[test]
    fn reverse_request_keeps_navigation_order_and_legacy_bounds() {
        let snapshot = snapshot(
            &[
                ReaderPageState::Error,
                ReaderPageState::Ready,
                ReaderPageState::Queued,
                ReaderPageState::Ready,
                ReaderPageState::Loading,
                ReaderPageState::Ready,
            ],
            Some(5),
        );

        let plan = plan_reader_schedule(
            &snapshot,
            ReaderScheduleCommand::Request {
                index: 3,
                cached: false,
                prefetch_page_count: 3,
            },
        )
        .expect("reverse request should plan");

        assert_eq!(
            plan.actions.last(),
            Some(&ReaderScheduleAction::Prefetch {
                pages: vec![2, 0],
                bounds_start: -5,
                bounds_end: 7,
            })
        );
    }

    #[test]
    fn empty_progressions_still_emit_direction_specific_prefetch_bounds() {
        let forward = plan_reader_schedule(
            &snapshot(&[ReaderPageState::Ready, ReaderPageState::Ready], Some(0)),
            ReaderScheduleCommand::Request {
                index: 1,
                cached: true,
                prefetch_page_count: 0,
            },
        )
        .expect("empty forward progression should plan");
        assert_eq!(
            forward.actions.last(),
            Some(&ReaderScheduleAction::Prefetch {
                pages: Vec::new(),
                bounds_start: -3,
                bounds_end: 6,
            })
        );

        let backward = plan_reader_schedule(
            &snapshot(&[ReaderPageState::Ready, ReaderPageState::Ready], Some(1)),
            ReaderScheduleCommand::Request {
                index: 0,
                cached: true,
                prefetch_page_count: 0,
            },
        )
        .expect("empty reverse progression should plan");
        assert_eq!(
            backward.actions.last(),
            Some(&ReaderScheduleAction::Prefetch {
                pages: Vec::new(),
                bounds_start: -5,
                bounds_end: 4,
            })
        );
    }

    #[test]
    fn retry_and_restart_preserve_the_legacy_effect_order() {
        let snapshot = snapshot(&[ReaderPageState::Error, ReaderPageState::Ready], Some(1));

        let retry = plan_reader_schedule(
            &snapshot,
            ReaderScheduleCommand::Retry {
                index: 0,
                original_image: true,
            },
        )
        .expect("retry should plan");
        assert_eq!(
            retry.actions,
            vec![
                ReaderScheduleAction::ResetOne { index: 0 },
                ReaderScheduleAction::RemoveCache { index: 0 },
                ReaderScheduleAction::Fetch {
                    index: 0,
                    force: true,
                    original_image: true,
                },
            ]
        );

        let restart = plan_reader_schedule(&snapshot, ReaderScheduleCommand::Restart)
            .expect("restart should plan");
        assert_eq!(
            restart.actions,
            vec![
                ReaderScheduleAction::ClearCache,
                ReaderScheduleAction::ResetAll,
            ]
        );
    }

    #[test]
    fn rejects_empty_oversized_and_out_of_bounds_snapshots() {
        assert_eq!(
            plan_reader_schedule(
                &ReaderScheduleSnapshot {
                    page_states: Vec::new(),
                    previous_index: None,
                },
                ReaderScheduleCommand::Restart,
            ),
            Err(ReaderScheduleError::EmptyPages)
        );

        let oversized = ReaderScheduleSnapshot {
            page_states: vec![ReaderPageState::Queued; MAX_READER_PAGE_COUNT + 1],
            previous_index: None,
        };
        assert_eq!(
            plan_reader_schedule(&oversized, ReaderScheduleCommand::Restart),
            Err(ReaderScheduleError::TooManyPages {
                actual: MAX_READER_PAGE_COUNT + 1,
                maximum: MAX_READER_PAGE_COUNT,
            })
        );

        let previous_out_of_bounds = snapshot(&[ReaderPageState::Queued], Some(1));
        assert_eq!(
            plan_reader_schedule(&previous_out_of_bounds, ReaderScheduleCommand::Restart),
            Err(ReaderScheduleError::PreviousIndexOutOfBounds {
                index: 1,
                page_count: 1,
            })
        );

        let valid = snapshot(&[ReaderPageState::Queued], None);
        assert_eq!(
            plan_reader_schedule(
                &valid,
                ReaderScheduleCommand::Retry {
                    index: 1,
                    original_image: false,
                },
            ),
            Err(ReaderScheduleError::PageIndexOutOfBounds {
                index: 1,
                page_count: 1,
            })
        );
        assert_eq!(
            plan_reader_schedule(
                &valid,
                ReaderScheduleCommand::Request {
                    index: 0,
                    cached: false,
                    prefetch_page_count: MAX_READER_PREFETCH_PAGE_COUNT + 1,
                },
            ),
            Err(ReaderScheduleError::PrefetchPageCountTooLarge {
                actual: MAX_READER_PREFETCH_PAGE_COUNT + 1,
                maximum: MAX_READER_PREFETCH_PAGE_COUNT,
            })
        );
    }

    #[test]
    fn every_error_has_a_stable_diagnostic_and_source_contract() {
        let errors = [
            ReaderScheduleError::EmptyPages,
            ReaderScheduleError::TooManyPages {
                actual: 2,
                maximum: 1,
            },
            ReaderScheduleError::PreviousIndexOutOfBounds {
                index: 2,
                page_count: 1,
            },
            ReaderScheduleError::PageIndexOutOfBounds {
                index: 3,
                page_count: 1,
            },
            ReaderScheduleError::PrefetchPageCountTooLarge {
                actual: 2,
                maximum: 1,
            },
        ];

        for error in errors {
            assert!(!error.to_string().is_empty());
            assert!(std::error::Error::source(&error).is_none());
        }
    }

    #[test]
    fn request_and_envelope_have_stable_json_and_cbor_contracts() {
        let request = ReaderScheduleRequest {
            version: READER_SCHEDULE_API_VERSION,
            snapshot: snapshot(&[ReaderPageState::Ready, ReaderPageState::Queued], Some(0)),
            command: ReaderScheduleCommand::Request {
                index: 1,
                cached: false,
                prefetch_page_count: 3,
            },
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request should encode as JSON"),
            json!({
                "version": 1,
                "snapshot": {
                    "pageStates": ["ready", "queued"],
                    "previousIndex": 0,
                },
                "command": {
                    "kind": "request",
                    "index": 1,
                    "cached": false,
                    "prefetchPageCount": 3,
                },
            })
        );
        let request_cbor = serde_cbor::to_vec(&request).expect("request should encode as CBOR");
        assert_eq!(hex(&request_cbor), REQUEST_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<ReaderScheduleRequest>(&request_cbor)
                .expect("request should decode from CBOR"),
            request
        );

        let plan = ReaderSchedulePlan {
            actions: vec![
                ReaderScheduleAction::SetPrevious { index: 1 },
                ReaderScheduleAction::UseCached { index: 1 },
                ReaderScheduleAction::Prefetch {
                    pages: Vec::new(),
                    bounds_start: -3,
                    bounds_end: 7,
                },
            ],
        };
        let envelope = ApiEnvelope::ok(
            READER_SCHEDULE_API_VERSION,
            READER_SCHEDULE_MODULE,
            plan.clone(),
        );
        let response_cbor = serde_cbor::to_vec(&envelope).expect("response should encode as CBOR");
        assert_eq!(hex(&response_cbor), RESPONSE_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<ReaderSchedulePlan>>(&response_cbor)
                .expect("response should decode from CBOR")
                .payload,
            Some(plan)
        );
    }

    fn snapshot(
        page_states: &[ReaderPageState],
        previous_index: Option<u32>,
    ) -> ReaderScheduleSnapshot {
        ReaderScheduleSnapshot {
            page_states: page_states.to_vec(),
            previous_index,
        }
    }

    fn hex(bytes: &[u8]) -> String {
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }

    const REQUEST_GOLDEN_CBOR_HEX: &str = concat!(
        "a36776657273696f6e0168736e617073686f74a26a706167655374617465738265726561647966",
        "7175657565646d70726576696f7573496e6465780067636f6d6d616e64a4646b696e6467726571",
        "7565737465696e6465780166636163686564f471707265666574636850616765436f756e7403",
    );
    const RESPONSE_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656e7265616465725363686564756c65677061796c6f",
        "6164a167616374696f6e7383a2646b696e646b73657450726576696f757365696e64657801a264",
        "6b696e646975736543616368656465696e64657801a4646b696e64687072656665746368657061",
        "676573806b626f756e647353746172742269626f756e6473456e6407656572726f72f6",
    );
}
