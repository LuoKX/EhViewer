//! Pure rules around Android-owned library persistence.
//!
//! Android retains Room, Paging, mutable URL builders, dialogs, HTTP, and model
//! mutation. This module owns FTS query shaping, `QuickSearch` equality, and local
//! favorite action planning over sanitized snapshots.

use serde::{Deserialize, Serialize};

use crate::contracts::RustError;

/// Protocol version of all library-rule requests and responses.
pub const LIBRARY_RULE_API_VERSION: u8 = 1;

/// Stable module identifier used by library-rule response envelopes.
pub const LIBRARY_RULE_MODULE: &str = "libraryRule";

const NOT_FAVORITED: i32 = -2;
const LOCAL_FAVORITED: i32 = -1;

/// Versioned input for one Room FTS query string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSearchQueryRequest {
    /// Protocol version; callers must use [`LIBRARY_RULE_API_VERSION`].
    pub version: u8,
    /// Raw Android search text retained inside the FTS wildcard query.
    pub keyword: String,
}

/// Complete query passed back to the Android-owned DAO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseSearchQueryPlan {
    /// Complete wildcard query passed to the Android-owned DAO.
    pub query: String,
}

/// Shapes the legacy prefix-and-suffix wildcard query without touching Room.
#[must_use]
pub fn plan_database_search_query(request: &DatabaseSearchQueryRequest) -> DatabaseSearchQueryPlan {
    DatabaseSearchQueryPlan {
        query: format!("*{}*", request.keyword),
    }
}

/// Portable fields participating in `QuickSearch` duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchSnapshot {
    /// Gallery-list mode discriminator.
    pub mode: i32,
    /// Category bit mask.
    pub category: i32,
    /// Optional query text.
    pub keyword: Option<String>,
    /// Advanced-search bit mask.
    pub advanced_search: i32,
    /// Minimum-rating filter.
    pub minimum_rating: i32,
    /// First page bound.
    pub page_from: i32,
    /// Last page bound.
    pub page_to: i32,
}

/// Versioned duplicate-detection request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchMatchRequest {
    /// Protocol version; callers must use [`LIBRARY_RULE_API_VERSION`].
    pub version: u8,
    /// Current mutable builder projection.
    pub current: QuickSearchSnapshot,
    /// Optional persisted row projection.
    pub saved: Option<QuickSearchSnapshot>,
}

/// Whether the saved row has exactly the same portable fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchMatchPlan {
    /// Whether all seven portable fields match exactly.
    pub matches: bool,
}

/// Compares exactly the seven fields used by the legacy builder.
#[must_use]
pub fn plan_quick_search_match(request: &QuickSearchMatchRequest) -> QuickSearchMatchPlan {
    QuickSearchMatchPlan {
        matches: request.saved.as_ref() == Some(&request.current),
    }
}

/// Versioned facts needed to plan one favorite-state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFavoriteMutationRequest {
    /// Protocol version; callers must use [`LIBRARY_RULE_API_VERSION`].
    pub version: u8,
    /// Requested local/cloud slot using the legacy `-2`, `-1`, or `0..=9` values.
    pub requested_slot: i32,
    /// Current local/cloud slot using the same legacy value space.
    pub current_slot: i32,
    /// Whether the gallery currently has an Android-owned local row.
    pub local_favorited: bool,
}

/// Android effect selected before Room or HTTP is touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalFavoriteAction {
    /// Remove the current remote favorite.
    RemoveCloud,
    /// Remove the Android-owned local favorite row.
    RemoveLocal,
    /// Add an Android-owned local favorite row.
    AddLocal,
    /// Set the requested remote favorite slot.
    SetCloud,
}

/// Complete favorite mutation plan applied by Android.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFavoriteMutationPlan {
    /// Single Android effect selected before mutation.
    pub action: LocalFavoriteAction,
    /// Favorite slot projected after the effect.
    pub next_favorite_slot: i32,
    /// Remote slot supplied only for [`LocalFavoriteAction::SetCloud`].
    pub cloud_slot: Option<i32>,
    /// Legacy added/removed result returned to the caller.
    pub added: bool,
}

/// Plans exactly one existing local/cloud favorite transition.
///
/// # Errors
///
/// Returns a sanitized [`RustError`] when the requested slot is outside
/// `-2`, `-1`, or `0..=9`.
pub fn plan_local_favorite_mutation(
    request: LocalFavoriteMutationRequest,
) -> Result<LocalFavoriteMutationPlan, RustError> {
    let plan = match request.requested_slot {
        NOT_FAVORITED if request.current_slot > LOCAL_FAVORITED => LocalFavoriteMutationPlan {
            action: LocalFavoriteAction::RemoveCloud,
            next_favorite_slot: if request.local_favorited {
                LOCAL_FAVORITED
            } else {
                NOT_FAVORITED
            },
            cloud_slot: None,
            added: false,
        },
        NOT_FAVORITED => LocalFavoriteMutationPlan {
            action: LocalFavoriteAction::RemoveLocal,
            next_favorite_slot: NOT_FAVORITED,
            cloud_slot: None,
            added: false,
        },
        LOCAL_FAVORITED => LocalFavoriteMutationPlan {
            action: if request.local_favorited {
                LocalFavoriteAction::RemoveLocal
            } else {
                LocalFavoriteAction::AddLocal
            },
            next_favorite_slot: match request.current_slot {
                NOT_FAVORITED => LOCAL_FAVORITED,
                LOCAL_FAVORITED => NOT_FAVORITED,
                slot => slot,
            },
            cloud_slot: None,
            added: !request.local_favorited,
        },
        slot @ 0..=9 => LocalFavoriteMutationPlan {
            action: LocalFavoriteAction::SetCloud,
            next_favorite_slot: slot,
            cloud_slot: Some(slot),
            added: true,
        },
        _ => {
            return Err(RustError::Error("Invalid favorite slot!".to_owned()));
        }
    };
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::{
        DatabaseSearchQueryRequest, LIBRARY_RULE_API_VERSION, LocalFavoriteAction,
        LocalFavoriteMutationRequest, QuickSearchMatchRequest, QuickSearchSnapshot,
        plan_database_search_query, plan_local_favorite_mutation, plan_quick_search_match,
    };

    #[test]
    fn shapes_history_and_local_favorite_fts_queries() {
        let request = DatabaseSearchQueryRequest {
            version: LIBRARY_RULE_API_VERSION,
            keyword: "artist name".to_owned(),
        };
        assert_eq!(plan_database_search_query(&request).query, "*artist name*");
    }

    #[test]
    fn quick_search_match_uses_only_the_seven_portable_fields() {
        let snapshot = quick_search();
        let mut request = QuickSearchMatchRequest {
            version: LIBRARY_RULE_API_VERSION,
            current: snapshot.clone(),
            saved: Some(snapshot),
        };
        assert!(plan_quick_search_match(&request).matches);
        request.saved.as_mut().expect("saved snapshot").page_to = 99;
        assert!(!plan_quick_search_match(&request).matches);
        request.saved = None;
        assert!(!plan_quick_search_match(&request).matches);
    }

    #[test]
    fn plans_local_cloud_add_remove_and_toggle_transitions() {
        let remove_cloud = plan(-2, 3, true);
        assert_eq!(remove_cloud.action, LocalFavoriteAction::RemoveCloud);
        assert_eq!(remove_cloud.next_favorite_slot, -1);

        let remove_local = plan(-1, -1, true);
        assert_eq!(remove_local.action, LocalFavoriteAction::RemoveLocal);
        assert_eq!(remove_local.next_favorite_slot, -2);
        assert!(!remove_local.added);

        let add_local_beside_cloud = plan(-1, 4, false);
        assert_eq!(add_local_beside_cloud.action, LocalFavoriteAction::AddLocal);
        assert_eq!(add_local_beside_cloud.next_favorite_slot, 4);
        assert!(add_local_beside_cloud.added);

        let set_cloud = plan(7, -2, false);
        assert_eq!(set_cloud.action, LocalFavoriteAction::SetCloud);
        assert_eq!(set_cloud.cloud_slot, Some(7));
    }

    #[test]
    fn rejects_invalid_favorite_slots_before_effects() {
        assert!(
            plan_local_favorite_mutation(LocalFavoriteMutationRequest {
                version: LIBRARY_RULE_API_VERSION,
                requested_slot: 10,
                current_slot: -2,
                local_favorited: false,
            })
            .is_err()
        );
    }

    fn plan(
        requested_slot: i32,
        current_slot: i32,
        local_favorited: bool,
    ) -> super::LocalFavoriteMutationPlan {
        plan_local_favorite_mutation(LocalFavoriteMutationRequest {
            version: LIBRARY_RULE_API_VERSION,
            requested_slot,
            current_slot,
            local_favorited,
        })
        .expect("valid favorite transition")
    }

    fn quick_search() -> QuickSearchSnapshot {
        QuickSearchSnapshot {
            mode: 0,
            category: 2,
            keyword: Some("query".to_owned()),
            advanced_search: 3,
            minimum_rating: 4,
            page_from: 5,
            page_to: 6,
        }
    }
}
