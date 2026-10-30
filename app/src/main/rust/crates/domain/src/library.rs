//! Pure rules around Android-owned library persistence.
//!
//! Android retains Room, Paging, mutable URL builders, dialogs, HTTP, and model
//! mutation. This module owns FTS query shaping, `QuickSearch` equality and import
//! planning, and local favorite action planning over sanitized snapshots.

use std::collections::HashSet;

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
        matches: quick_search_matches(&request.current, request.saved.as_ref()),
    }
}

/// Shared comparison for one legacy query or a batched creation preflight.
#[must_use]
pub fn quick_search_matches(
    current: &QuickSearchSnapshot,
    saved: Option<&QuickSearchSnapshot>,
) -> bool {
    saved == Some(current)
}

/// Versioned names and row count needed to plan one `QuickSearch` import.
///
/// Names are user-authored values and must never be included in errors or logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchImportRequest {
    /// Protocol version; callers must use [`LIBRARY_RULE_API_VERSION`].
    pub version: u8,
    /// Exact names already present in the Android-owned current database.
    pub current_names: Vec<String>,
    /// Exact names read from the imported database in source order.
    pub imported_names: Vec<String>,
    /// Current Android row count represented with JVM signed-`Int` semantics.
    pub current_count: i32,
}

/// Source-aligned decisions for one Android-owned `QuickSearch` import.
///
/// Each entry corresponds to the imported name at the same index. `None` skips
/// that row. `Some(position)` tells Android to clear the source Room ID and insert
/// the row at `position`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickSearchImportPlan {
    /// Optional destination positions aligned with imported source order.
    pub positions: Vec<Option<i32>>,
}

/// Plans exact-name deduplication and contiguous reindexing for one import.
///
/// Only names present in `current_names` are excluded. Accepted imported names
/// are deliberately not added to that set, so duplicates within the imported
/// source remain present and retain source order. Position increments use signed
/// 32-bit wrapping arithmetic to match JVM `Int` overflow behavior exactly.
#[must_use]
pub fn plan_quick_search_import(request: &QuickSearchImportRequest) -> QuickSearchImportPlan {
    let current_names: HashSet<&str> = request.current_names.iter().map(String::as_str).collect();
    let mut next_position = request.current_count;
    let positions = request
        .imported_names
        .iter()
        .map(|name| {
            if current_names.contains(name.as_str()) {
                None
            } else {
                let position = next_position;
                next_position = next_position.wrapping_add(1);
                Some(position)
            }
        })
        .collect();
    QuickSearchImportPlan { positions }
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
    use std::fmt::Write as _;

    use crate::contracts::ApiEnvelope;

    use super::{
        DatabaseSearchQueryRequest, LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE,
        LocalFavoriteAction, LocalFavoriteMutationRequest, QuickSearchImportPlan,
        QuickSearchImportRequest, QuickSearchMatchRequest, QuickSearchSnapshot,
        plan_database_search_query, plan_local_favorite_mutation, plan_quick_search_import,
        plan_quick_search_match,
    };

    const QUICK_SEARCH_IMPORT_REQUEST_GOLDEN: &str = "a46776657273696f6e016c63757272656e744e616d6573816473616d656d696d706f727465644e616d6573836473616d65636e6577636e65776c63757272656e74436f756e7401";
    const QUICK_SEARCH_IMPORT_RESPONSE_GOLDEN: &str = "a46776657273696f6e01666d6f64756c656b6c69627261727952756c65677061796c6f6164a169706f736974696f6e7383f60102656572726f72f6";

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
    fn quick_search_import_compares_only_exact_current_names() {
        let plan = plan_quick_search_import(&QuickSearchImportRequest {
            version: LIBRARY_RULE_API_VERSION,
            current_names: vec!["same".to_owned(), "Case".to_owned(), String::new()],
            imported_names: vec![
                "same".to_owned(),
                "SAME".to_owned(),
                "new".to_owned(),
                "new".to_owned(),
                String::new(),
                "Case".to_owned(),
            ],
            current_count: 3,
        });

        assert_eq!(
            plan.positions,
            vec![None, Some(3), Some(4), Some(5), None, None]
        );
    }

    #[test]
    fn quick_search_import_positions_wrap_like_jvm_ints() {
        let plan = plan_quick_search_import(&QuickSearchImportRequest {
            version: LIBRARY_RULE_API_VERSION,
            current_names: Vec::new(),
            imported_names: vec!["one".to_owned(), "two".to_owned(), "three".to_owned()],
            current_count: i32::MAX,
        });

        assert_eq!(
            plan.positions,
            vec![Some(i32::MAX), Some(i32::MIN), Some(i32::MIN + 1)]
        );
    }

    #[test]
    fn quick_search_import_wire_is_versioned_and_source_aligned() {
        let request = QuickSearchImportRequest {
            version: LIBRARY_RULE_API_VERSION,
            current_names: vec!["same".to_owned()],
            imported_names: vec!["same".to_owned(), "new".to_owned(), "new".to_owned()],
            current_count: 1,
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request should serialize"),
            serde_json::json!({
                "version": 1,
                "currentNames": ["same"],
                "importedNames": ["same", "new", "new"],
                "currentCount": 1,
            })
        );

        let encoded = serde_cbor::to_vec(&request).expect("request should encode");
        let response = serde_cbor::to_vec(&ApiEnvelope::ok(
            LIBRARY_RULE_API_VERSION,
            LIBRARY_RULE_MODULE,
            plan_quick_search_import(&request),
        ))
        .expect("response should encode");
        let to_hex = |bytes: &[u8]| {
            bytes.iter().fold(String::new(), |mut output, byte| {
                write!(output, "{byte:02x}").expect("writing to a String should succeed");
                output
            })
        };
        assert_eq!(to_hex(&encoded), QUICK_SEARCH_IMPORT_REQUEST_GOLDEN);
        assert_eq!(to_hex(&response), QUICK_SEARCH_IMPORT_RESPONSE_GOLDEN);
        assert_eq!(
            serde_cbor::from_slice::<QuickSearchImportRequest>(&encoded)
                .expect("request should decode"),
            request
        );
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<QuickSearchImportPlan>>(&response)
                .expect("response should decode")
                .payload,
            Some(plan_quick_search_import(&request))
        );
        assert_eq!(
            plan_quick_search_import(&request).positions,
            vec![None, Some(1), Some(2)]
        );
    }

    #[test]
    fn plans_local_cloud_add_remove_and_toggle_transitions() {
        let remove_cloud = plan(-2, 3, true);
        assert_eq!(remove_cloud.action, LocalFavoriteAction::RemoveCloud);
        assert_eq!(remove_cloud.next_favorite_slot, -1);

        let remove_cloud_without_local = plan(-2, 3, false);
        assert_eq!(
            remove_cloud_without_local.action,
            LocalFavoriteAction::RemoveCloud
        );
        assert_eq!(remove_cloud_without_local.next_favorite_slot, -2);

        let remove_absent_local = plan(-2, -2, false);
        assert_eq!(remove_absent_local.action, LocalFavoriteAction::RemoveLocal);
        assert_eq!(remove_absent_local.next_favorite_slot, -2);

        let remove_local = plan(-1, -1, true);
        assert_eq!(remove_local.action, LocalFavoriteAction::RemoveLocal);
        assert_eq!(remove_local.next_favorite_slot, -2);
        assert!(!remove_local.added);

        let add_only_local = plan(-1, -2, false);
        assert_eq!(add_only_local.action, LocalFavoriteAction::AddLocal);
        assert_eq!(add_only_local.next_favorite_slot, -1);

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
