//! Pure download-catalog sorting and filtering.
//!
//! Android retains Room entities, Compose state, labels, settings persistence,
//! and list mutation. This module receives immutable display snapshots and
//! returns only ordered or visible gallery identifiers.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// Protocol version of download-catalog requests and responses.
pub const DOWNLOAD_CATALOG_API_VERSION: u8 = 1;

/// Stable module identifier used by download-catalog response envelopes.
pub const DOWNLOAD_CATALOG_MODULE: &str = "downloadCatalog";

/// Immutable display facts for one Android-owned download row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCatalogEntry {
    /// Stable gallery identifier used to project the final order.
    pub gallery_id: i64,
    /// Optional primary title from the Android-owned row.
    pub title: Option<String>,
    /// Optional Japanese title from the Android-owned row.
    pub title_jpn: Option<String>,
    /// Legacy signed timestamp used by the catalog ordering.
    pub time: i64,
    /// Legacy signed page count used by the catalog ordering.
    pub pages: i32,
    /// Optional custom download label.
    pub label: Option<String>,
    /// Android download-state discriminator.
    pub state: i32,
    /// Source-ordered artist values used by artist grouping.
    pub artists: Vec<String>,
    /// Source-ordered simple tags searched by the keyword filter.
    pub simple_tags: Vec<String>,
}

/// Primary download sort field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadSortField {
    /// Order by gallery identifier.
    GalleryId,
    /// Order by the selected display title.
    Title,
    /// Order by the legacy timestamp.
    Time,
    /// Order by page count, with timestamp as the legacy tie-breaker.
    Pages,
}

/// Primary download sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadSortOrder {
    /// Keep the primary comparison in ascending order.
    Ascending,
    /// Reverse only the primary comparison.
    Descending,
}

/// Versioned request for one complete catalog ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSortRequest {
    /// Protocol version; callers must use [`DOWNLOAD_CATALOG_API_VERSION`].
    pub version: u8,
    /// Complete immutable catalog snapshot.
    pub entries: Vec<DownloadCatalogEntry>,
    /// Selected primary sort field.
    pub field: DownloadSortField,
    /// Selected primary sort direction.
    pub order: DownloadSortOrder,
    /// Whether optional labels form the first stable grouping key.
    pub group_by_download_label: bool,
    /// Whether a non-empty Japanese title is preferred.
    pub prefer_japanese_title: bool,
}

/// Complete ordered identifier plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSortPlan {
    /// Gallery identifiers in the complete selected order.
    pub gallery_ids: Vec<i64>,
}

/// Download grouping mode used by filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadFilterMode {
    /// Match the Android-owned custom label.
    Custom,
    /// Match an artist value, including the unlabeled sentinel.
    Artist,
}

/// Versioned request for one complete catalog filter operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFilterRequest {
    /// Protocol version; callers must use [`DOWNLOAD_CATALOG_API_VERSION`].
    pub version: u8,
    /// Complete immutable catalog snapshot.
    pub entries: Vec<DownloadCatalogEntry>,
    /// Selected custom-label or artist grouping mode.
    pub mode: DownloadFilterMode,
    /// Selected label; null and empty retain their legacy meanings.
    pub label: Option<String>,
    /// Required state, or `-1` to accept every state.
    pub state: i32,
    /// Case-insensitive title and tag search text.
    pub keyword: String,
}

/// Visible identifiers in the same stable order as the request snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFilterPlan {
    /// Visible gallery identifiers in request order.
    pub gallery_ids: Vec<i64>,
}

/// Returns one stable complete ordering without mutating the request snapshot.
#[must_use]
pub fn sort_download_catalog(request: &DownloadSortRequest) -> DownloadSortPlan {
    let mut entries: Vec<&DownloadCatalogEntry> = request.entries.iter().collect();
    entries.sort_by(|left, right| compare_entries(left, right, request));
    DownloadSortPlan {
        gallery_ids: entries.into_iter().map(|entry| entry.gallery_id).collect(),
    }
}

/// Returns visible identifiers without reordering the request snapshot.
#[must_use]
pub fn filter_download_catalog(request: &DownloadFilterRequest) -> DownloadFilterPlan {
    DownloadFilterPlan {
        gallery_ids: request
            .entries
            .iter()
            .filter(|entry| matches_filter(entry, request))
            .map(|entry| entry.gallery_id)
            .collect(),
    }
}

fn compare_entries(
    left: &DownloadCatalogEntry,
    right: &DownloadCatalogEntry,
    request: &DownloadSortRequest,
) -> Ordering {
    if request.group_by_download_label {
        let grouped = compare_optional_utf16(left.label.as_deref(), right.label.as_deref());
        if grouped != Ordering::Equal {
            return grouped;
        }
    }

    let mut primary = match request.field {
        DownloadSortField::GalleryId => left.gallery_id.cmp(&right.gallery_id),
        DownloadSortField::Title => compare_utf16(
            suitable_title(left, request.prefer_japanese_title),
            suitable_title(right, request.prefer_japanese_title),
        ),
        DownloadSortField::Time => left.time.cmp(&right.time),
        DownloadSortField::Pages => left.pages.cmp(&right.pages),
    };
    if request.order == DownloadSortOrder::Descending {
        primary = primary.reverse();
    }
    if primary == Ordering::Equal && request.field == DownloadSortField::Pages {
        right.time.cmp(&left.time)
    } else {
        primary
    }
}

fn suitable_title(entry: &DownloadCatalogEntry, prefer_japanese: bool) -> &str {
    let (preferred, fallback) = if prefer_japanese {
        (&entry.title_jpn, &entry.title)
    } else {
        (&entry.title, &entry.title_jpn)
    };
    preferred
        .as_deref()
        .filter(|title| !title.is_empty())
        .or(fallback.as_deref())
        .unwrap_or("")
}

fn compare_optional_utf16(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(left), Some(right)) => compare_utf16(left, right),
    }
}

fn compare_utf16(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn matches_filter(entry: &DownloadCatalogEntry, request: &DownloadFilterRequest) -> bool {
    let group_matches = match request.mode {
        DownloadFilterMode::Custom => {
            request.label.as_deref() == Some("") || entry.label == request.label
        }
        DownloadFilterMode::Artist => match request.label.as_deref() {
            None => entry.artists.is_empty(),
            Some("") => true,
            Some(label) => entry.artists.iter().any(|artist| artist == label),
        },
    };
    group_matches
        && (request.state == -1 || entry.state == request.state)
        && entry_matches_keyword(entry, &request.keyword)
}

fn entry_matches_keyword(entry: &DownloadCatalogEntry, keyword: &str) -> bool {
    entry
        .title
        .as_deref()
        .is_some_and(|title| contains_ignore_case(title, keyword))
        || entry
            .title_jpn
            .as_deref()
            .is_some_and(|title| contains_ignore_case(title, keyword))
        || entry
            .simple_tags
            .iter()
            .any(|tag| contains_ignore_case(tag, keyword))
}

fn contains_ignore_case(value: &str, keyword: &str) -> bool {
    value.to_lowercase().contains(&keyword.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::{
        DOWNLOAD_CATALOG_API_VERSION, DownloadCatalogEntry, DownloadFilterMode,
        DownloadFilterRequest, DownloadSortField, DownloadSortOrder, DownloadSortRequest,
        filter_download_catalog, sort_download_catalog,
    };

    #[test]
    fn sorts_pages_with_time_descending_ties_and_optional_label_groups() {
        let entries = vec![
            entry(1, 10, 100, Some("b")),
            entry(2, 10, 200, Some("a")),
            entry(3, 5, 300, None),
            entry(4, 10, 300, Some("a")),
        ];
        let mut request = DownloadSortRequest {
            version: DOWNLOAD_CATALOG_API_VERSION,
            entries,
            field: DownloadSortField::Pages,
            order: DownloadSortOrder::Ascending,
            group_by_download_label: false,
            prefer_japanese_title: false,
        };
        assert_eq!(
            sort_download_catalog(&request).gallery_ids,
            vec![3, 4, 2, 1]
        );

        request.group_by_download_label = true;
        assert_eq!(
            sort_download_catalog(&request).gallery_ids,
            vec![3, 4, 2, 1]
        );
    }

    #[test]
    fn title_sort_uses_the_selected_fallback_and_utf16_order() {
        let mut first = entry(1, 1, 1, None);
        first.title = Some("z".to_owned());
        first.title_jpn = Some("あ".to_owned());
        let mut second = entry(2, 1, 1, None);
        second.title = Some("a".to_owned());
        let entries = vec![first, second];
        let mut request = DownloadSortRequest {
            version: DOWNLOAD_CATALOG_API_VERSION,
            entries,
            field: DownloadSortField::Title,
            order: DownloadSortOrder::Ascending,
            group_by_download_label: false,
            prefer_japanese_title: false,
        };
        assert_eq!(sort_download_catalog(&request).gallery_ids, vec![2, 1]);
        request.prefer_japanese_title = true;
        assert_eq!(sort_download_catalog(&request).gallery_ids, vec![2, 1]);
    }

    #[test]
    fn filters_custom_artist_state_and_keyword_without_reordering() {
        let mut first = entry(1, 1, 1, Some("saved"));
        first.state = 3;
        first.title = Some("Example Title".to_owned());
        first.artists = vec!["alice".to_owned()];
        let mut second = entry(2, 1, 1, None);
        second.state = 4;
        second.simple_tags = vec!["language:Example".to_owned()];
        let entries = vec![first, second];

        let custom = DownloadFilterRequest {
            version: DOWNLOAD_CATALOG_API_VERSION,
            entries: entries.clone(),
            mode: DownloadFilterMode::Custom,
            label: Some("saved".to_owned()),
            state: 3,
            keyword: "example".to_owned(),
        };
        assert_eq!(filter_download_catalog(&custom).gallery_ids, vec![1]);

        let artist = DownloadFilterRequest {
            version: DOWNLOAD_CATALOG_API_VERSION,
            entries,
            mode: DownloadFilterMode::Artist,
            label: None,
            state: -1,
            keyword: "example".to_owned(),
        };
        assert_eq!(filter_download_catalog(&artist).gallery_ids, vec![2]);
    }

    fn entry(gallery_id: i64, pages: i32, time: i64, label: Option<&str>) -> DownloadCatalogEntry {
        DownloadCatalogEntry {
            gallery_id,
            title: None,
            title_jpn: None,
            time,
            pages,
            label: label.map(str::to_owned),
            state: 0,
            artists: Vec::new(),
            simple_tags: Vec::new(),
        }
    }
}
