//! Scalar-only `ComicInfo` refresh and archive preparation policy; effects remain Android-owned.
use crate::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Legacy archive validation error retained by the Kotlin adapter.
pub const INVALID_PAGES: &str = "Archive expected page count must be positive";
/// Legacy required-directory error retained by the Kotlin adapter.
pub const MISSING_DIRECTORY: &str = "Required value was null.";

/// Closed production operations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComicInfoOperation {
    /// Validate expected pages, inspect metadata, then keep/create/rewrite it.
    Prepare,
    /// Prefer known gallery pages; inspect metadata only when required.
    ResolveExport,
    /// Optional metadata refresh, followed by fresh XML and artist persistence.
    Write,
}
/// Android's read observation. Failed/malformed reads retain the legacy missing semantics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComicInfoMetadataState {
    /// No filesystem/XML observation has been requested yet.
    Unread,
    /// No decodable metadata is available.
    Missing,
    /// Existing metadata was decoded; page count may be nonpositive.
    Present,
}
/// Fixed-size V1 observations. No XML, gallery title, token, path or credentials cross this API.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoPolicyRequest {
    /// Contract version.
    pub version: u8,
    /// Operation being planned.
    pub operation: ComicInfoOperation,
    /// Whether the Android download directory is available.
    pub has_directory: bool,
    /// Whether the gallery already has detailed metadata.
    pub detailed_gallery: bool,
    /// Direct-write caller's refresh preference.
    pub fetch_metadata: bool,
    /// Current gallery page count, without filtering negative/zero observations.
    pub gallery_pages: i32,
    /// Pinned operation page count; present only for preparation.
    pub expected_pages: Option<i32>,
    /// Current XML read observation.
    pub metadata_state: ComicInfoMetadataState,
    /// Existing XML page count, present exactly when metadata state is Present.
    pub metadata_pages: Option<i32>,
}
/// Closed Android execution actions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComicInfoAction {
    /// No directory: perform no metadata/network/database effects.
    Skip,
    /// Observe XML once and request a final plan; never apply partial effects.
    ReadMetadata,
    /// Apply the resolved positive gallery page count.
    ResolvePages,
    /// Build/write fresh XML and synchronize download artists after successful write.
    Create,
    /// Copy only `PageCount` in existing XML; do not regenerate fields or artists.
    Rewrite,
    /// Keep existing XML and apply the pinned gallery page count.
    Keep,
}
/// Network failure policy; cancellation always propagates in the Android executor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ComicInfoFetchMode {
    /// No metadata network request.
    None,
    /// Direct metadata refresh: every failure propagates.
    Propagate,
    /// Archive preparation: log ordinary exceptions and continue; cancellation propagates.
    Continue,
}
/// Complete plan, containing no mutable metadata object or platform resource.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComicInfoPolicyPlan {
    /// Closed action.
    pub action: ComicInfoAction,
    /// Positive count to apply after network effects; absent for reads/direct writes.
    pub page_count: Option<i32>,
    /// Whether and how to execute a metadata request before persistence.
    pub fetch_mode: ComicInfoFetchMode,
}

/// Decide the complete next operation with fixed errors and constant work/allocation.
#[must_use]
pub fn build_envelope(request: &ComicInfoPolicyRequest) -> ApiEnvelope<ComicInfoPolicyPlan> {
    if request.version != 1
        || (request.metadata_state == ComicInfoMetadataState::Present)
            != request.metadata_pages.is_some()
        || (request.operation == ComicInfoOperation::Prepare) != request.expected_pages.is_some()
    {
        return ApiEnvelope::err(
            1,
            "comicInfoPolicy",
            error("Invalid ComicInfo policy input"),
        );
    }
    let result = match request.operation {
        ComicInfoOperation::Prepare => prepare(request),
        ComicInfoOperation::ResolveExport => resolve_export(request),
        ComicInfoOperation::Write => Ok(plan(
            if request.has_directory {
                ComicInfoAction::Create
            } else {
                ComicInfoAction::Skip
            },
            None,
            if request.has_directory && !request.detailed_gallery && request.fetch_metadata {
                ComicInfoFetchMode::Propagate
            } else {
                ComicInfoFetchMode::None
            },
        )),
    };
    match result {
        Ok(value) => ApiEnvelope::ok(1, "comicInfoPolicy", value),
        Err(value) => ApiEnvelope::err(1, "comicInfoPolicy", value),
    }
}
fn prepare(request: &ComicInfoPolicyRequest) -> Result<ComicInfoPolicyPlan, RustError> {
    let pages = request
        .expected_pages
        .filter(|&pages| pages > 0)
        .ok_or_else(|| error(INVALID_PAGES))?;
    if !request.has_directory {
        return Err(error(MISSING_DIRECTORY));
    }
    match request.metadata_state {
        ComicInfoMetadataState::Unread => Ok(plan(
            ComicInfoAction::ReadMetadata,
            None,
            ComicInfoFetchMode::None,
        )),
        ComicInfoMetadataState::Missing => Ok(plan(
            ComicInfoAction::Create,
            Some(pages),
            if request.detailed_gallery {
                ComicInfoFetchMode::None
            } else {
                ComicInfoFetchMode::Continue
            },
        )),
        ComicInfoMetadataState::Present => Ok(plan(
            if request.metadata_pages == Some(pages) {
                ComicInfoAction::Keep
            } else {
                ComicInfoAction::Rewrite
            },
            Some(pages),
            ComicInfoFetchMode::None,
        )),
    }
}
fn resolve_export(request: &ComicInfoPolicyRequest) -> Result<ComicInfoPolicyPlan, RustError> {
    if request.gallery_pages > 0 {
        return Ok(plan(
            ComicInfoAction::ResolvePages,
            Some(request.gallery_pages),
            ComicInfoFetchMode::None,
        ));
    }
    if !request.has_directory {
        return Err(error(INVALID_PAGES));
    }
    if request.metadata_state == ComicInfoMetadataState::Unread {
        return Ok(plan(
            ComicInfoAction::ReadMetadata,
            None,
            ComicInfoFetchMode::None,
        ));
    }
    let pages = request
        .metadata_pages
        .filter(|&pages| pages > 0)
        .ok_or_else(|| error(INVALID_PAGES))?;
    Ok(plan(
        ComicInfoAction::ResolvePages,
        Some(pages),
        ComicInfoFetchMode::None,
    ))
}
fn plan(
    action: ComicInfoAction,
    page_count: Option<i32>,
    fetch_mode: ComicInfoFetchMode,
) -> ComicInfoPolicyPlan {
    ComicInfoPolicyPlan {
        action,
        page_count,
        fetch_mode,
    }
}
fn error(message: &str) -> RustError {
    RustError::Error(message.into())
}

#[cfg(test)]
mod tests;
