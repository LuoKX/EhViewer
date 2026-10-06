//! Bounded local download metadata observation and persistence policy.
//!
//! Android reads sidecars with the existing Rust ComicInfo/SpiderInfo codecs. Their
//! decoded tag/artist lists are already canonical; this owner preserves them,
//! including duplicate artists, rather than projecting or deduplicating them again.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Maximum rows in either phase.
pub const MAX_ROWS: usize = 32;
/// Maximum combined tag/artist strings in one batch.
pub const MAX_STRINGS: usize = 16_384;
/// Maximum combined UTF-8 tag/artist bytes in one batch.
pub const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum encoded request/response size.
pub const MAX_WIRE_BYTES: usize = 3 * 1024 * 1024;

/// Closed two-phase protocol.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Decide which sidecars Android should observe.
    Observe,
    /// Decide patches after all requested observations have completed.
    Patch,
}
/// Immutable missing-field facts; no Room entities or gallery identity cross JNI.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Current pages; zero alone means missing.
    pub pages: i32,
    /// Null tags, not an empty list, mean missing.
    pub tags_missing: bool,
    /// Empty relation means missing artists.
    pub artists_missing: bool,
    /// Android can resolve a download directory.
    pub has_directory: bool,
}
/// Facts from the existing `ComicInfo` decoder, without URLs or titles.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicFacts {
    /// Decoded `PageCount`, including zero/negative counts.
    pub pages: i32,
    /// Existing decoder's canonical simple-tag projection.
    pub simple_tags: Option<Vec<String>>,
    /// Existing decoder's penciller list; duplicates and empty lists are retained.
    pub penciller: Option<Vec<String>>,
}
/// One completed sidecar observation. Missing/unreadable/corrupt all map to None.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    /// Source snapshot index.
    pub index: usize,
    /// Decoded `ComicInfo` facts, if available. Omit missing nullable objects to
    /// match the definite-length Android CBOR encoder's default-field wire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comic: Option<ComicFacts>,
    /// Decoded .ehviewer pages only after `ComicInfo` is missing and pages are zero.
    /// Identity is deliberately not compared, matching the legacy read path.
    pub spider_pages: Option<i32>,
}
/// Versioned batch request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Exact protocol version.
    pub version: u8,
    /// Current phase.
    pub phase: Phase,
    /// Pinned Android-owned row snapshots in source order.
    pub entries: Vec<Snapshot>,
    /// Empty in Observe; one result per read plan in Patch.
    pub observations: Vec<Observation>,
}
/// Closed observation action: `ComicInfo` first, conditional .ehviewer second.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPlan {
    /// Source snapshot index.
    pub index: usize,
    /// Read .ehviewer only if `ComicInfo` is missing/unreadable/corrupt.
    pub spider_if_comic_missing: bool,
}
/// Field patch plus exact Android persistence intent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    /// Source snapshot index.
    pub index: usize,
    /// Page count to apply, or no page change.
    pub pages: Option<i32>,
    /// Distinguishes clearing tags to null from not modifying tags.
    pub update_tags: bool,
    /// New canonical tags, including null.
    pub simple_tags: Option<Vec<String>>,
    /// New artist names; null means no relation update.
    pub artists: Option<Vec<String>>,
    /// Persist the gallery row even if the assigned values are unchanged.
    pub persist_gallery: bool,
    /// Persist the artist relation even if the decoded list is empty.
    pub persist_artists: bool,
}
/// Complete phase result, validated by Android before any effects.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Read actions; empty in Patch.
    pub reads: Vec<ReadPlan>,
    /// Patches; empty in Observe. No-op rows are omitted.
    pub patches: Vec<Patch>,
}

/// Plan a complete phase with fixed payload-free errors and bounded allocations.
#[must_use]
pub fn build_envelope(request: &Request) -> ApiEnvelope<Plan> {
    if request.version != 1 || request.entries.len() > MAX_ROWS {
        return invalid();
    }
    let reads = read_plan(&request.entries);
    let valid = match request.phase {
        Phase::Observe => request.observations.is_empty(),
        Phase::Patch => {
            request.observations.len() == reads.len()
                && request.observations.iter().zip(&reads).all(|(fact, read)| {
                    fact.index == read.index
                        && (fact.spider_pages.is_none()
                            || (fact.comic.is_none() && read.spider_if_comic_missing))
                })
        }
    } && within_budget(&request.observations);
    if !valid {
        return invalid();
    }
    let plan = match request.phase {
        Phase::Observe => Plan {
            reads,
            patches: Vec::new(),
        },
        Phase::Patch => Plan {
            reads: Vec::new(),
            patches: request
                .observations
                .iter()
                .filter_map(|fact| patch(&request.entries[fact.index], fact))
                .collect(),
        },
    };
    ApiEnvelope::ok(1, "downloadMetadata", plan)
}
fn invalid() -> ApiEnvelope<Plan> {
    ApiEnvelope::err(
        1,
        "downloadMetadata",
        RustError::Error("Invalid download metadata input".into()),
    )
}
fn read_plan(entries: &[Snapshot]) -> Vec<ReadPlan> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.has_directory && (entry.pages == 0 || entry.tags_missing || entry.artists_missing)
        })
        .map(|(index, entry)| ReadPlan {
            index,
            spider_if_comic_missing: entry.pages == 0,
        })
        .collect()
}
fn within_budget(observations: &[Observation]) -> bool {
    let mut count = 0;
    let mut bytes = 0;
    for text in observations
        .iter()
        .filter_map(|fact| fact.comic.as_ref())
        .flat_map(|comic| [comic.simple_tags.as_ref(), comic.penciller.as_ref()])
        .flatten()
        .flatten()
    {
        count += 1;
        bytes += text.len();
        if count > MAX_STRINGS || bytes > MAX_TEXT_BYTES {
            return false;
        }
    }
    true
}
fn patch(entry: &Snapshot, fact: &Observation) -> Option<Patch> {
    let mut result = Patch {
        index: fact.index,
        pages: None,
        update_tags: false,
        simple_tags: None,
        artists: None,
        persist_gallery: false,
        persist_artists: false,
    };
    if let Some(comic) = &fact.comic {
        if entry.pages == 0 || entry.tags_missing {
            result.pages = Some(comic.pages);
            result.update_tags = true;
            result.simple_tags.clone_from(&comic.simple_tags);
            result.persist_gallery = true;
        }
        if entry.artists_missing {
            result.artists.clone_from(&comic.penciller);
            result.persist_artists = comic.penciller.is_some();
        }
    } else if let Some(pages) = fact.spider_pages {
        result.pages = Some(pages);
        result.persist_gallery = true;
    }
    (result.persist_gallery || result.persist_artists).then_some(result)
}

#[cfg(test)]
mod tests;
