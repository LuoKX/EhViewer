//! Portable reader budgets, completion counters, quota and advertisement rules.

use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// V1 closed reader policy inputs. Android owns all effects and exception objects.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReaderPolicyRequest {
    /// Plan budgets once, before decoding can exhaust the Java heap.
    Memory {
        /// Java heap limit in bytes.
        app_max_memory: i64,
        /// Device RAM, or non-positive when unknown.
        total_memory: i64,
    },
    /// Reduce one page transition under the Android page-state lock.
    PageTransition {
        /// Total page count.
        pages: i32,
        /// Finished page count before this transition.
        finished: i32,
        /// Terminal page count before this transition.
        downloaded: i32,
        /// Prior closed page state.
        old_state: PageState,
        /// New closed page state.
        new_state: PageState,
    },
    /// Evaluate the legacy last-nine-page detection rule and tag presence.
    Ads {
        /// Zero-based index, absent for a tags-only query.
        index: Option<i32>,
        /// Page count, present exactly with the index.
        pages: Option<i32>,
        /// Snapshot of the strip-extraneous-ads setting.
        enabled: bool,
        /// Unmodified gallery tags.
        tags: Option<Vec<String>>,
    },
    /// Identify the exact known quota placeholder URL without HTTP.
    Quota {
        /// Candidate image URL; never included in errors.
        url: String,
    },
    /// Convert the preference level to KiB/second, preserving Float rounding.
    SpeedLevel {
        /// Stored exponent; level three disables the watchdog.
        level: i32,
    },
}

/// Closed page states matching the Android `SpiderQueen` integers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageState {
    /// Not requested.
    None,
    /// Download in progress.
    Downloading,
    /// Successfully downloaded.
    Finished,
    /// Terminal failure.
    Failed,
}

/// Listener intent after a page transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageNotification {
    /// No page listener notification.
    None,
    /// Notify page success.
    Success,
    /// Notify page failure.
    Failure,
}

/// Gallery completion intent after a page transition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageCompletion {
    /// No completion effect.
    None,
    /// Start the lazy archive job.
    Archive,
    /// Notify failed gallery completion.
    Failure,
}

/// Complete V1 result, validated by Kotlin before effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ReaderPolicyPlan {
    /// Positive cache and decoder budgets.
    Memory {
        /// Maximum cache bytes.
        cache_size: i32,
        /// Concurrent decoders.
        decode_concurrency: i32,
        /// Total decode attempts, including the first.
        decode_attempts: i32,
    },
    /// Updated counters and effects.
    PageTransition {
        /// Successfully downloaded pages.
        finished: i32,
        /// Terminal pages.
        downloaded: i32,
        /// Single-page listener intent.
        notification: PageNotification,
        /// Gallery completion intent.
        completion: PageCompletion,
    },
    /// Independent tag and detection flags.
    Ads {
        /// Whether QR detection is enabled for this index.
        detect: bool,
        /// Whether any tag contains the exact legacy marker.
        has_ads: bool,
    },
    /// Quota marker decision.
    Quota {
        /// Whether Android must throw its quota exception.
        exceeded: bool,
    },
    /// Speed threshold.
    SpeedLevel {
        /// KiB/second; zero disables low-speed detection.
        kib_per_second: i32,
    },
}

/// Saturates one image's accounting without overflow, retaining oversized pages.
///
/// # Errors
/// Returns a fixed error for a non-positive cache size.
#[allow(
    clippy::missing_panics_doc,
    reason = "clamping guarantees the signed conversion"
)]
pub fn image_entry_size(allocation_size: i64, cache_size: i32) -> Result<i32, RustError> {
    if cache_size <= 0 {
        return Err(invalid());
    }
    Ok(
        i32::try_from(allocation_size.clamp(1, i64::from(cache_size)))
            .expect("bounded cache entry"),
    )
}

/// Executes one pure policy request.
///
/// # Errors
/// Returns a fixed, input-free error for inconsistent counters or index facts.
#[allow(
    clippy::cast_possible_truncation,
    reason = "Float-to-Int saturation matches Kotlin roundToInt"
)]
#[allow(
    clippy::missing_panics_doc,
    reason = "clamping guarantees the signed cache conversion"
)]
pub fn plan(request: &ReaderPolicyRequest) -> Result<ReaderPolicyPlan, RustError> {
    Ok(match request {
        ReaderPolicyRequest::Memory {
            app_max_memory,
            total_memory,
        } => {
            let heap = (app_max_memory / 4).max(1);
            let device = if *total_memory > 0 {
                total_memory / 8
            } else {
                heap
            };
            ReaderPolicyPlan::Memory {
                cache_size: i32::try_from(heap.min(device).clamp(1, 128 * 1024 * 1024))
                    .expect("bounded cache"),
                decode_concurrency: 2,
                decode_attempts: 2,
            }
        }
        ReaderPolicyRequest::PageTransition {
            pages,
            finished,
            downloaded,
            old_state,
            new_state,
        } => {
            if *pages <= 0 || *finished < 0 || finished > downloaded || downloaded > pages {
                return Err(invalid());
            }
            let done = |state| matches!(state, PageState::Finished | PageState::Failed);
            let finished = finished + i32::from(*new_state == PageState::Finished)
                - i32::from(*old_state == PageState::Finished);
            let downloaded = downloaded + i32::from(done(*new_state)) - i32::from(done(*old_state));
            if finished < 0 || finished > downloaded || downloaded > *pages {
                return Err(invalid());
            }
            ReaderPolicyPlan::PageTransition {
                finished,
                downloaded,
                notification: match new_state {
                    PageState::Failed => PageNotification::Failure,
                    PageState::Finished => PageNotification::Success,
                    _ => PageNotification::None,
                },
                completion: if downloaded != *pages {
                    PageCompletion::None
                } else if finished == *pages {
                    PageCompletion::Archive
                } else {
                    PageCompletion::Failure
                },
            }
        }
        ReaderPolicyRequest::Ads {
            index,
            pages,
            enabled,
            tags,
        } => {
            if index.is_some() != pages.is_some() {
                return Err(invalid());
            }
            ReaderPolicyPlan::Ads {
                detect: *enabled
                    && index
                        .zip(*pages)
                        .is_some_and(|(index, pages)| i64::from(index) > i64::from(pages) - 10),
                has_ads: tags
                    .as_ref()
                    .is_some_and(|tags| tags.iter().any(|tag| tag.contains("extraneous ads"))),
            }
        }
        ReaderPolicyRequest::Quota { url } => ReaderPolicyPlan::Quota {
            exceeded: [
                "https://ehgt.org/g/509.gif",
                "https://ehgt.org/g/509s.gif",
                "https://exhentai.org/img/509.gif",
                "https://exhentai.org/img/509s.gif",
            ]
            .contains(&url.as_str()),
        },
        ReaderPolicyRequest::SpeedLevel { level } => ReaderPolicyPlan::SpeedLevel {
            kib_per_second: if *level == 3 {
                0
            } else {
                2_f32.powi(*level).round() as i32
            },
        },
    })
}

/// Builds the V1 transport envelope, rejecting unsupported versions.
#[must_use]
pub fn build_envelope(version: u8, request: &ReaderPolicyRequest) -> ApiEnvelope<ReaderPolicyPlan> {
    match if version == 1 {
        plan(request)
    } else {
        Err(invalid())
    } {
        Ok(plan) => ApiEnvelope::ok(1, "readerPolicy", plan),
        Err(error) => ApiEnvelope::err(1, "readerPolicy", error),
    }
}

fn invalid() -> RustError {
    RustError::Error("Invalid reader policy input".into())
}

#[cfg(test)]
mod tests;
