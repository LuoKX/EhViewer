//! Portable repair and normalization rules for Android-owned settings.
//!
//! Android retains `DataStore`, locale detection, preference mutation, and all
//! resulting side effects. This module receives sanitized snapshots and returns
//! complete repair or clamp plans.

use serde::{Deserialize, Serialize};

/// Protocol version of all settings-rule requests and responses.
pub const SETTINGS_RULE_API_VERSION: u8 = 1;
/// Stable module identifier used by settings-rule response envelopes.
pub const SETTINGS_RULE_MODULE: &str = "settingsRule";

const DOWNLOAD_FILTER_MODE_ARTIST: i32 = 1;
const READ_CACHE_SIZE_MIN_MIB: i32 = 320;
const READ_CACHE_SIZE_MAX_MIB: i32 = 5120;

/// Versioned facts needed to repair startup settings.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRepairRequest {
    /// Protocol version; callers must use [`SETTINGS_RULE_API_VERSION`].
    pub version: u8,
    /// Whether Android resolved the current locale as Chinese.
    pub chinese_locale: bool,
    /// Whether the tag-translation preference is absent.
    pub show_tag_translations_absent: bool,
    /// Current download-filter mode discriminator.
    pub download_filter_mode: i32,
    /// Optional recent label persisted by Android.
    pub recent_download_label: Option<String>,
    /// Current orientation mode discriminator.
    pub orientation_mode: i32,
    /// Complete Android-supported orientation modes.
    pub valid_orientation_modes: Vec<i32>,
}

/// Complete startup repair decisions applied by Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRepairPlan {
    /// Whether Android should enable tag translations.
    pub enable_tag_translations: bool,
    /// Whether Android should reset the missing recent label.
    pub reset_recent_download_label: bool,
    /// Whether Android should remove the invalid orientation value.
    pub remove_orientation_mode: bool,
}

/// Plans startup repairs without reading or mutating `DataStore`.
#[must_use]
pub fn plan_settings_repair(request: &SettingsRepairRequest) -> SettingsRepairPlan {
    SettingsRepairPlan {
        enable_tag_translations: request.chinese_locale && request.show_tag_translations_absent,
        reset_recent_download_label: request.download_filter_mode == DOWNLOAD_FILTER_MODE_ARTIST
            && request.recent_download_label.is_none(),
        remove_orientation_mode: !request
            .valid_orientation_modes
            .contains(&request.orientation_mode),
    }
}

/// Versioned configured cache-size snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCacheSizeRequest {
    /// Protocol version; callers must use [`SETTINGS_RULE_API_VERSION`].
    pub version: u8,
    /// Android-configured cache size in mebibytes.
    pub configured_mib: i32,
}

/// Bounded read-cache size returned to Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCacheSizePlan {
    /// Cache size in mebibytes, clamped to the supported range.
    pub size_mib: i32,
}

/// Clamps one configured cache size without reading platform state.
#[must_use]
pub fn plan_read_cache_size(request: &ReadCacheSizeRequest) -> ReadCacheSizePlan {
    ReadCacheSizePlan {
        size_mib: request
            .configured_mib
            .clamp(READ_CACHE_SIZE_MIN_MIB, READ_CACHE_SIZE_MAX_MIB),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_only_invalid_or_missing_settings() {
        let plan = plan_settings_repair(&SettingsRepairRequest {
            version: SETTINGS_RULE_API_VERSION,
            chinese_locale: true,
            show_tag_translations_absent: true,
            download_filter_mode: DOWNLOAD_FILTER_MODE_ARTIST,
            recent_download_label: None,
            orientation_mode: 99,
            valid_orientation_modes: vec![0, 1, 2],
        });

        assert!(plan.enable_tag_translations);
        assert!(plan.reset_recent_download_label);
        assert!(plan.remove_orientation_mode);
    }

    #[test]
    fn preserves_valid_settings() {
        let plan = plan_settings_repair(&SettingsRepairRequest {
            version: SETTINGS_RULE_API_VERSION,
            chinese_locale: false,
            show_tag_translations_absent: false,
            download_filter_mode: 0,
            recent_download_label: Some("label".to_owned()),
            orientation_mode: 1,
            valid_orientation_modes: vec![0, 1, 2],
        });

        assert_eq!(
            plan,
            SettingsRepairPlan {
                enable_tag_translations: false,
                reset_recent_download_label: false,
                remove_orientation_mode: false,
            }
        );
    }

    #[test]
    fn clamps_read_cache_size() {
        assert_eq!(
            plan_read_cache_size(&ReadCacheSizeRequest {
                version: SETTINGS_RULE_API_VERSION,
                configured_mib: 1,
            })
            .size_mib,
            READ_CACHE_SIZE_MIN_MIB
        );
        assert_eq!(
            plan_read_cache_size(&ReadCacheSizeRequest {
                version: SETTINGS_RULE_API_VERSION,
                configured_mib: 9000,
            })
            .size_mib,
            READ_CACHE_SIZE_MAX_MIB
        );
    }
}
