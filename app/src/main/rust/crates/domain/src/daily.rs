//! Pure daily-check eligibility decisions.
//!
//! Android retains `WorkManager` scheduling, sign-in state, persistence, HTTP,
//! notifications, and the platform clock. This module receives only sanitized
//! facts and returns the UTC day plus whether the network check should run.

use serde::{Deserialize, Serialize};

/// Protocol version of the daily-check request and response.
pub const DAILY_CHECK_API_VERSION: u8 = 1;

/// Stable module identifier used by the daily-check response envelope.
pub const DAILY_CHECK_MODULE: &str = "dailyCheck";

const SECONDS_PER_DAY: i64 = 86_400;

/// Versioned facts captured by Android for one daily-check operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheckRequest {
    /// Protocol version; callers must use [`DAILY_CHECK_API_VERSION`].
    pub version: u8,
    /// Whether the application currently has an authenticated session.
    pub signed_in: bool,
    /// UTC day persisted after the most recent successful event response.
    pub last_dawn_days: i32,
    /// Whole Unix seconds captured once from the platform clock.
    pub now_epoch_seconds: i64,
}

/// Complete pure decision applied by Android before performing network effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheckPlan {
    /// Current UTC epoch day using the legacy truncating division contract.
    pub today: i32,
    /// Whether Android should request the event pane now.
    pub should_check: bool,
}

/// Plans one daily-check operation without reading platform state or causing effects.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the legacy Kotlin Long.toInt contract wraps epoch days outside the i32 range"
)]
pub fn plan_daily_check(request: &DailyCheckRequest) -> DailyCheckPlan {
    let today = (request.now_epoch_seconds / SECONDS_PER_DAY) as i32;
    DailyCheckPlan {
        today,
        should_check: request.signed_in && request.last_dawn_days != today,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DAILY_CHECK_API_VERSION, DailyCheckRequest, plan_daily_check};

    #[test]
    fn plans_only_signed_in_users_on_a_new_utc_day() {
        let mut request = DailyCheckRequest {
            version: DAILY_CHECK_API_VERSION,
            signed_in: false,
            last_dawn_days: 10,
            now_epoch_seconds: 11 * 86_400,
        };
        assert!(!plan_daily_check(&request).should_check);

        request.signed_in = true;
        assert!(plan_daily_check(&request).should_check);

        request.last_dawn_days = 11;
        assert!(!plan_daily_check(&request).should_check);
    }

    #[test]
    fn preserves_the_legacy_truncating_epoch_day_contract() {
        let request = DailyCheckRequest {
            version: DAILY_CHECK_API_VERSION,
            signed_in: true,
            last_dawn_days: -1,
            now_epoch_seconds: -1,
        };

        assert_eq!(plan_daily_check(&request).today, 0);
    }

    #[test]
    fn request_has_a_stable_camel_case_contract() {
        let request = DailyCheckRequest {
            version: DAILY_CHECK_API_VERSION,
            signed_in: true,
            last_dawn_days: 10,
            now_epoch_seconds: 950_400,
        };

        assert_eq!(
            serde_json::to_value(request).expect("request should encode as JSON"),
            json!({
                "version": 1,
                "signedIn": true,
                "lastDawnDays": 10,
                "nowEpochSeconds": 950_400,
            })
        );
    }
}
