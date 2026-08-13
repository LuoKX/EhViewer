//! Pure Gallery list and comment filtering plans.
//!
//! Android retains Room filter rows, mutable Gallery models, comment storage,
//! and Android ICU regular-expression evaluation. This module receives immutable
//! snapshots plus aligned ICU match facts and owns every keep/drop decision.

use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// Protocol version of Gallery-filter requests and responses.
pub const GALLERY_FILTER_API_VERSION: u8 = 1;
/// Stable module identifier used by Gallery-filter response envelopes.
pub const GALLERY_FILTER_MODULE: &str = "galleryFilter";

const FILTER_MODE_TITLE: i32 = 0;
const FILTER_MODE_UPLOADER: i32 = 1;
const FILTER_MODE_TAG: i32 = 2;
const FILTER_MODE_TAG_NAMESPACE: i32 = 3;
const FILTER_MODE_COMMENTER: i32 = 4;
const FILTER_MODE_COMMENT: i32 = 5;

/// One complete Gallery-filter operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryFilterOperation {
    /// Filters title and uploader before optional metadata enrichment.
    PreEnrichment,
    /// Rechecks uploader and filters tags after optional metadata enrichment.
    PostEnrichment,
    /// Filters one complete Gallery comment list.
    Comment,
}

/// Immutable Android Room filter data used by the portable planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilterSnapshot {
    /// Stable `FilterMode` database discriminator in the range `0..=5`.
    pub mode: i32,
    /// Complete stored filter text.
    pub text: String,
    /// Whether this filter is enabled.
    pub enabled: bool,
}

/// Portable fields needed to filter one Android-owned Gallery row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilterGallery {
    /// Optional primary title; absence has the legacy empty-title meaning.
    pub title: Option<String>,
    /// Optional uploader.
    pub uploader: Option<String>,
    /// Optional source-ordered simple tags.
    pub tags: Option<Vec<String>>,
}

/// Portable fields needed to filter one Android-owned Gallery comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilterComment {
    /// Signed legacy comment score.
    pub score: i32,
    /// Whether the comment belongs to the Gallery uploader.
    pub uploader: bool,
    /// Optional commenter name; absence has the legacy empty-name meaning.
    pub commenter: Option<String>,
}

/// Versioned request for one complete batch filtering operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilterRequest {
    /// Protocol version; callers must use [`GALLERY_FILTER_API_VERSION`].
    pub version: u8,
    /// Selected batch operation.
    pub operation: GalleryFilterOperation,
    /// Complete source-ordered immutable filter snapshot.
    pub filters: Vec<GalleryFilterSnapshot>,
    /// Complete Gallery subjects for pre- or post-enrichment operations.
    pub galleries: Vec<GalleryFilterGallery>,
    /// Complete comment subjects for a comment operation.
    pub comments: Vec<GalleryFilterComment>,
    /// Android-owned score threshold, present only for a comment operation.
    pub score_threshold: Option<i32>,
    /// Optional Android ICU content-match facts indexed by comment, then filter.
    ///
    /// A comment request without facts asks Rust which enabled content filters
    /// must be evaluated in source order. A request with facts finalizes the
    /// keep/drop plan; `None` marks a filter which Rust did not ask Android to run.
    pub comment_content_matches: Option<Vec<Vec<Option<bool>>>>,
}

/// Atomic keep/drop plan aligned with the selected operation's subjects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryFilterPlan {
    /// Echoed operation used by Android to reject response drift.
    pub operation: GalleryFilterOperation,
    /// One keep decision for every request subject, in request order.
    pub keep: Vec<bool>,
    /// Whether any enabled tag or tag-namespace filter requires metadata.
    pub needs_tags: bool,
    /// Source filter indexes Android ICU must evaluate for each comment.
    ///
    /// This is populated only by comment preflight. Android evaluates each row
    /// in order and stops after the first match or compilation failure.
    pub comment_evaluation_filter_indices: Vec<Vec<usize>>,
}

/// Sanitized Gallery-filter request validation failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GalleryFilterError(&'static str);

impl Display for GalleryFilterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for GalleryFilterError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterMode {
    Title,
    Uploader,
    Tag,
    TagNamespace,
    Commenter,
    Comment,
}

impl TryFrom<i32> for FilterMode {
    type Error = GalleryFilterError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            FILTER_MODE_TITLE => Ok(Self::Title),
            FILTER_MODE_UPLOADER => Ok(Self::Uploader),
            FILTER_MODE_TAG => Ok(Self::Tag),
            FILTER_MODE_TAG_NAMESPACE => Ok(Self::TagNamespace),
            FILTER_MODE_COMMENTER => Ok(Self::Commenter),
            FILTER_MODE_COMMENT => Ok(Self::Comment),
            _ => Err(GalleryFilterError("Invalid Gallery filter mode")),
        }
    }
}

/// Validates and plans one complete Gallery filtering batch.
///
/// Android ICU facts are consumed only for enabled comment-content filters. A
/// true fact on any other filter kind is ignored, so Rust remains the sole owner
/// of enabled-state and mode selection.
///
/// # Errors
///
/// Returns a fixed body-free failure for unsupported versions, invalid modes,
/// operation-shape drift, or a comment/filter fact alignment mismatch.
pub fn plan_gallery_filter(
    request: &GalleryFilterRequest,
) -> Result<GalleryFilterPlan, GalleryFilterError> {
    if request.version != GALLERY_FILTER_API_VERSION {
        return Err(GalleryFilterError("Unsupported Gallery filter API version"));
    }

    let filters = request
        .filters
        .iter()
        .map(|filter| FilterMode::try_from(filter.mode).map(|mode| (mode, filter)))
        .collect::<Result<Vec<_>, _>>()?;
    validate_request_shape(request)?;

    let needs_tags = filters.iter().any(|(mode, filter)| {
        filter.enabled && matches!(mode, FilterMode::Tag | FilterMode::TagNamespace)
    });
    let (keep, comment_evaluation_filter_indices) = match request.operation {
        GalleryFilterOperation::PreEnrichment => (
            request
                .galleries
                .iter()
                .map(|gallery| !is_pre_enrichment_filtered(gallery, &filters))
                .collect(),
            Vec::new(),
        ),
        GalleryFilterOperation::PostEnrichment => (
            request
                .galleries
                .iter()
                .map(|gallery| !is_post_enrichment_filtered(gallery, &filters))
                .collect(),
            Vec::new(),
        ),
        GalleryFilterOperation::Comment => {
            let threshold = request
                .score_threshold
                .ok_or(GalleryFilterError("Invalid Gallery comment filter shape"))?;
            if let Some(content_matches) = request.comment_content_matches.as_ref() {
                let keep = request
                    .comments
                    .iter()
                    .zip(content_matches)
                    .map(|(comment, facts)| keep_comment(comment, threshold, &filters, facts))
                    .collect::<Result<Vec<_>, _>>()?;
                (keep, Vec::new())
            } else {
                let evaluation_indices = request
                    .comments
                    .iter()
                    .map(|comment| comment_evaluation_indices(comment, threshold, &filters))
                    .collect();
                (Vec::new(), evaluation_indices)
            }
        }
    };

    Ok(GalleryFilterPlan {
        operation: request.operation,
        keep,
        needs_tags,
        comment_evaluation_filter_indices,
    })
}

fn validate_request_shape(request: &GalleryFilterRequest) -> Result<(), GalleryFilterError> {
    match request.operation {
        GalleryFilterOperation::PreEnrichment | GalleryFilterOperation::PostEnrichment => {
            if !request.comments.is_empty()
                || request.score_threshold.is_some()
                || request.comment_content_matches.is_some()
            {
                return Err(GalleryFilterError("Invalid Gallery list filter shape"));
            }
        }
        GalleryFilterOperation::Comment => {
            if !request.galleries.is_empty()
                || request.score_threshold.is_none()
                || request
                    .comment_content_matches
                    .as_ref()
                    .is_some_and(|matches| {
                        matches.len() != request.comments.len()
                            || matches
                                .iter()
                                .any(|facts| facts.len() != request.filters.len())
                    })
            {
                return Err(GalleryFilterError("Invalid Gallery comment filter shape"));
            }
        }
    }
    Ok(())
}

fn is_pre_enrichment_filtered(
    gallery: &GalleryFilterGallery,
    filters: &[(FilterMode, &GalleryFilterSnapshot)],
) -> bool {
    filters.iter().any(|(mode, filter)| {
        filter.enabled
            && match mode {
                FilterMode::Title => {
                    contains_ignore_case(gallery.title.as_deref().unwrap_or_default(), &filter.text)
                }
                FilterMode::Uploader => gallery.uploader.as_ref() == Some(&filter.text),
                FilterMode::Tag
                | FilterMode::TagNamespace
                | FilterMode::Commenter
                | FilterMode::Comment => false,
            }
    })
}

fn is_post_enrichment_filtered(
    gallery: &GalleryFilterGallery,
    filters: &[(FilterMode, &GalleryFilterSnapshot)],
) -> bool {
    filters.iter().any(|(mode, filter)| {
        filter.enabled
            && match mode {
                FilterMode::Uploader => gallery.uploader.as_ref() == Some(&filter.text),
                FilterMode::Tag => gallery.tags.as_ref().is_some_and(|tags| {
                    let lowered_filter = filter.text.to_lowercase();
                    tags.iter().any(|tag| matches_tag(tag, &lowered_filter))
                }),
                FilterMode::TagNamespace => gallery.tags.as_ref().is_some_and(|tags| {
                    let lowered_filter = filter.text.to_lowercase();
                    tags.iter()
                        .any(|tag| tag_namespace(tag) == Some(lowered_filter.as_str()))
                }),
                FilterMode::Title | FilterMode::Commenter | FilterMode::Comment => false,
            }
    })
}

fn comment_evaluation_indices(
    comment: &GalleryFilterComment,
    threshold: i32,
    filters: &[(FilterMode, &GalleryFilterSnapshot)],
) -> Vec<usize> {
    if comment.uploader {
        return Vec::new();
    }
    if comment.score <= threshold {
        return Vec::new();
    }

    let commenter = comment.commenter.as_deref().unwrap_or_default();
    let commenter_filtered = filters.iter().any(|(mode, filter)| {
        filter.enabled && *mode == FilterMode::Commenter && filter.text == commenter
    });
    if commenter_filtered {
        return Vec::new();
    }

    filters
        .iter()
        .enumerate()
        .filter_map(|(index, (mode, filter))| {
            (filter.enabled && *mode == FilterMode::Comment).then_some(index)
        })
        .collect()
}

fn keep_comment(
    comment: &GalleryFilterComment,
    threshold: i32,
    filters: &[(FilterMode, &GalleryFilterSnapshot)],
    content_matches: &[Option<bool>],
) -> Result<bool, GalleryFilterError> {
    let evaluation_indices = comment_evaluation_indices(comment, threshold, filters);
    if evaluation_indices.is_empty() {
        if content_matches.iter().any(Option::is_some) {
            return Err(GalleryFilterError("Unexpected Gallery comment match fact"));
        }
        let commenter = comment.commenter.as_deref().unwrap_or_default();
        let commenter_filtered = filters.iter().any(|(mode, filter)| {
            filter.enabled && *mode == FilterMode::Commenter && filter.text == commenter
        });
        return Ok(comment.uploader || (comment.score > threshold && !commenter_filtered));
    }

    for (index, fact) in content_matches.iter().enumerate() {
        let should_evaluate = evaluation_indices.contains(&index);
        if !should_evaluate {
            if fact.is_some() {
                return Err(GalleryFilterError("Unexpected Gallery comment match fact"));
            }
            continue;
        }

        let matched = fact.ok_or(GalleryFilterError("Missing Gallery comment match fact"))?;
        if matched {
            if content_matches[index + 1..].iter().any(Option::is_some) {
                return Err(GalleryFilterError("Unexpected Gallery comment match fact"));
            }
            return Ok(false);
        }
    }
    Ok(true)
}

fn matches_tag(tag: &str, filter: &str) -> bool {
    let (tag_namespace, tag_name) = split_tag(tag);
    let (filter_namespace, filter_name) = split_tag(filter);
    if tag_namespace.is_some() && filter_namespace.is_some() && tag_namespace != filter_namespace {
        false
    } else {
        tag_name == filter_name
    }
}

fn tag_namespace(tag: &str) -> Option<&str> {
    split_tag(tag).0
}

fn split_tag(tag: &str) -> (Option<&str>, &str) {
    tag.split_once(':')
        .map_or((None, tag), |(namespace, name)| (Some(namespace), name))
}

fn contains_ignore_case(value: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let value = value.chars().collect::<Vec<_>>();
    let pattern = pattern.chars().collect::<Vec<_>>();
    value.len() >= pattern.len()
        && value.windows(pattern.len()).any(|window| {
            window
                .iter()
                .zip(&pattern)
                .all(|(left, right)| chars_equal_ignore_case(*left, *right))
        })
}

fn chars_equal_ignore_case(left: char, right: char) -> bool {
    if left == right {
        return true;
    }
    let left_upper = simple_uppercase(left);
    let right_upper = simple_uppercase(right);
    left_upper == right_upper || simple_lowercase(left_upper) == simple_lowercase(right_upper)
}

fn simple_uppercase(value: char) -> char {
    let mut uppercase = value.to_uppercase();
    let first = uppercase.next().unwrap_or(value);
    if uppercase.next().is_none() {
        first
    } else {
        value
    }
}

fn simple_lowercase(value: char) -> char {
    value.to_lowercase().next().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use crate::contracts::ApiEnvelope;

    use super::*;

    #[test]
    fn pre_enrichment_owns_title_uploader_and_needs_tags() {
        let request = GalleryFilterRequest {
            version: GALLERY_FILTER_API_VERSION,
            operation: GalleryFilterOperation::PreEnrichment,
            filters: vec![
                filter(FILTER_MODE_TITLE, "needle", true),
                filter(FILTER_MODE_UPLOADER, "blocked", true),
                filter(FILTER_MODE_TAG, "tag", true),
                filter(FILTER_MODE_COMMENT, "ignored", true),
            ],
            galleries: vec![
                gallery(Some("A NEEDLE title"), None, None),
                gallery(None, Some("blocked"), None),
                gallery(Some("kept"), None, None),
            ],
            comments: vec![],
            score_threshold: None,
            comment_content_matches: None,
        };

        let plan = plan_gallery_filter(&request).expect("valid pre-enrichment request");

        assert_eq!(plan.operation, GalleryFilterOperation::PreEnrichment);
        assert_eq!(plan.keep, vec![false, false, true]);
        assert!(plan.needs_tags);
        assert!(plan.comment_evaluation_filter_indices.is_empty());
    }

    #[test]
    fn pre_enrichment_preserves_empty_and_unicode_ignore_case_quirks() {
        assert!(contains_ignore_case("anything", ""));
        assert!(contains_ignore_case("Final ς", "Σ"));
        assert!(contains_ignore_case("İ", "i"));
        assert!(!contains_ignore_case("ß", "s"));
        assert!(!contains_ignore_case("short", "a longer pattern"));

        let request = list_request(
            GalleryFilterOperation::PreEnrichment,
            vec![filter(FILTER_MODE_TITLE, "", true)],
            vec![gallery(None, None, None)],
        );
        assert_eq!(
            plan_gallery_filter(&request)
                .expect("empty title filter remains valid")
                .keep,
            vec![false]
        );
    }

    #[test]
    fn post_enrichment_rechecks_uploader_and_preserves_tag_semantics() {
        assert!(matches_tag("female:tag", "female:tag"));
        assert!(!matches_tag("female:tag", "male:tag"));
        assert!(matches_tag("female:tag", "tag"));
        assert!(matches_tag("tag", "female:tag"));
        assert_eq!(tag_namespace(":tag"), Some(""));
        assert_eq!(tag_namespace("tag"), None);

        let request = list_request(
            GalleryFilterOperation::PostEnrichment,
            vec![
                filter(FILTER_MODE_UPLOADER, "blocked", true),
                filter(FILTER_MODE_TAG, "FEMALE:TAG", true),
                filter(FILTER_MODE_TAG_NAMESPACE, "GROUP", true),
                filter(FILTER_MODE_TITLE, "ignored", true),
            ],
            vec![
                gallery(None, Some("blocked"), None),
                gallery(None, None, Some(vec!["female:tag"])),
                gallery(None, None, Some(vec!["group:other"])),
                gallery(None, None, Some(vec!["GROUP:other"])),
                gallery(None, None, Some(vec!["other:tag"])),
            ],
        );

        let plan = plan_gallery_filter(&request).expect("valid post-enrichment request");

        assert_eq!(plan.keep, vec![false, false, false, true, true]);
        assert!(plan.needs_tags);
    }

    #[test]
    fn comment_plan_owns_uploader_threshold_commenter_and_content_precedence() {
        let filters = vec![
            filter(FILTER_MODE_COMMENTER, "alice", true),
            filter(FILTER_MODE_COMMENT, "icu", true),
            filter(FILTER_MODE_COMMENT, "disabled", false),
            filter(FILTER_MODE_TITLE, "not-comment", true),
        ];
        let comments = vec![
            comment(-100, true, Some("alice")),
            comment(10, false, Some("nobody")),
            comment(11, false, Some("alice")),
            comment(11, false, Some("bob")),
            comment(11, false, None),
        ];
        let preflight = comment_request(filters.clone(), comments.clone(), 10, None);

        let preflight_plan =
            plan_gallery_filter(&preflight).expect("valid comment preflight request");

        assert!(preflight_plan.keep.is_empty());
        assert_eq!(
            preflight_plan.comment_evaluation_filter_indices,
            vec![vec![], vec![], vec![], vec![1], vec![1]]
        );

        let request = comment_request(
            filters,
            comments,
            10,
            Some(vec![
                vec![None, None, None, None],
                vec![None, None, None, None],
                vec![None, None, None, None],
                vec![None, Some(true), None, None],
                vec![None, Some(false), None, None],
            ]),
        );

        let plan = plan_gallery_filter(&request).expect("valid comment request");

        assert_eq!(plan.operation, GalleryFilterOperation::Comment);
        assert_eq!(plan.keep, vec![true, false, false, false, true]);
        assert!(!plan.needs_tags);
        assert!(plan.comment_evaluation_filter_indices.is_empty());
    }

    #[test]
    fn commenter_empty_text_matches_an_absent_commenter() {
        let request = GalleryFilterRequest {
            version: GALLERY_FILTER_API_VERSION,
            operation: GalleryFilterOperation::Comment,
            filters: vec![filter(FILTER_MODE_COMMENTER, "", true)],
            galleries: vec![],
            comments: vec![comment(1, false, None)],
            score_threshold: Some(0),
            comment_content_matches: Some(vec![vec![None]]),
        };

        assert_eq!(
            plan_gallery_filter(&request)
                .expect("valid absent-commenter request")
                .keep,
            vec![false]
        );
    }

    #[test]
    fn disabled_filters_and_empty_batches_keep_every_subject() {
        let disabled = list_request(
            GalleryFilterOperation::PreEnrichment,
            vec![filter(FILTER_MODE_TITLE, "drop", false)],
            vec![gallery(Some("drop"), None, None)],
        );
        let empty = list_request(GalleryFilterOperation::PostEnrichment, vec![], vec![]);

        assert_eq!(
            plan_gallery_filter(&disabled)
                .expect("disabled filters are valid")
                .keep,
            vec![true]
        );
        let empty_plan = plan_gallery_filter(&empty).expect("empty batches are valid");
        assert!(empty_plan.keep.is_empty());
        assert!(!empty_plan.needs_tags);
    }

    #[test]
    fn validation_rejects_versions_modes_operation_shapes_and_fact_drift() {
        let mut request = list_request(GalleryFilterOperation::PreEnrichment, vec![], vec![]);
        request.version = 2;
        assert_error(&request, "Unsupported Gallery filter API version");

        request.version = GALLERY_FILTER_API_VERSION;
        request.filters = vec![filter(99, "invalid", false)];
        assert_error(&request, "Invalid Gallery filter mode");

        request.filters.clear();
        request.comments.push(comment(1, false, None));
        assert_error(&request, "Invalid Gallery list filter shape");

        let mut comments = GalleryFilterRequest {
            version: GALLERY_FILTER_API_VERSION,
            operation: GalleryFilterOperation::Comment,
            filters: vec![filter(FILTER_MODE_COMMENT, "pattern", true)],
            galleries: vec![],
            comments: vec![comment(1, false, None)],
            score_threshold: Some(0),
            comment_content_matches: Some(vec![]),
        };
        assert_error(&comments, "Invalid Gallery comment filter shape");

        comments.comment_content_matches = Some(vec![vec![]]);
        assert_error(&comments, "Invalid Gallery comment filter shape");

        comments.comment_content_matches = Some(vec![vec![Some(false)]]);
        comments.galleries.push(gallery(None, None, None));
        assert_error(&comments, "Invalid Gallery comment filter shape");

        comments.galleries.clear();
        comments.score_threshold = None;
        assert_error(&comments, "Invalid Gallery comment filter shape");
    }

    #[test]
    fn comment_finalization_rejects_eager_missing_and_wrong_filter_facts() {
        let filters = vec![
            filter(FILTER_MODE_COMMENT, "first", true),
            filter(FILTER_MODE_COMMENT, "second", true),
            filter(FILTER_MODE_TITLE, "not-comment", true),
        ];
        let comments = vec![comment(1, false, Some("user"))];

        let eager = comment_request(
            filters.clone(),
            comments.clone(),
            0,
            Some(vec![vec![Some(true), Some(false), None]]),
        );
        assert_error(&eager, "Unexpected Gallery comment match fact");

        let missing = comment_request(
            filters.clone(),
            comments.clone(),
            0,
            Some(vec![vec![Some(false), None, None]]),
        );
        assert_error(&missing, "Missing Gallery comment match fact");

        let wrong_filter = comment_request(
            filters,
            comments,
            0,
            Some(vec![vec![Some(false), Some(false), Some(false)]]),
        );
        assert_error(&wrong_filter, "Unexpected Gallery comment match fact");
    }

    #[test]
    fn v1_request_and_response_cbor_match_the_kotlin_goldens() {
        let request = list_request(
            GalleryFilterOperation::PreEnrichment,
            vec![filter(FILTER_MODE_TITLE, "Needle", true)],
            vec![gallery(Some("Title"), Some("user"), None)],
        );
        let envelope = ApiEnvelope::ok(
            GALLERY_FILTER_API_VERSION,
            GALLERY_FILTER_MODULE,
            GalleryFilterPlan {
                operation: GalleryFilterOperation::PreEnrichment,
                keep: vec![true],
                needs_tags: false,
                comment_evaluation_filter_indices: vec![],
            },
        );

        assert_eq!(
            encode_hex(&request),
            "a76776657273696f6e01696f7065726174696f6e6d707265456e726963686d656e746766696c7465727381a3646d6f6465006474657874664e6565646c6567656e61626c6564f56967616c6c657269657381a3657469746c65655469746c656875706c6f6164657264757365726474616773f668636f6d6d656e7473806e73636f72655468726573686f6c64f675636f6d6d656e74436f6e74656e744d617463686573f6"
        );
        assert_eq!(
            encode_hex(&envelope),
            "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e6d707265456e726963686d656e74646b65657081f5696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
        );
    }

    #[test]
    fn post_and_two_phase_comment_cbor_match_the_kotlin_goldens() {
        let post_request = list_request(
            GalleryFilterOperation::PostEnrichment,
            vec![filter(FILTER_MODE_TAG, "female:tag", true)],
            vec![gallery(None, None, Some(vec!["female:tag", "group:value"]))],
        );
        let post_envelope = ApiEnvelope::ok(
            GALLERY_FILTER_API_VERSION,
            GALLERY_FILTER_MODULE,
            GalleryFilterPlan {
                operation: GalleryFilterOperation::PostEnrichment,
                keep: vec![false],
                needs_tags: true,
                comment_evaluation_filter_indices: vec![],
            },
        );
        let filters = vec![
            filter(FILTER_MODE_COMMENTER, "alice", true),
            filter(FILTER_MODE_COMMENT, "pattern", true),
        ];
        let comments = vec![
            comment(1, true, Some("alice")),
            comment(20, false, Some("bob")),
        ];
        let comment_preflight_request =
            comment_request(filters.clone(), comments.clone(), 10, None);
        let comment_preflight_envelope = ApiEnvelope::ok(
            GALLERY_FILTER_API_VERSION,
            GALLERY_FILTER_MODULE,
            GalleryFilterPlan {
                operation: GalleryFilterOperation::Comment,
                keep: vec![],
                needs_tags: false,
                comment_evaluation_filter_indices: vec![vec![], vec![1]],
            },
        );
        let comment_final_request = comment_request(
            filters,
            comments,
            10,
            Some(vec![vec![None, None], vec![None, Some(false)]]),
        );
        let comment_final_envelope = ApiEnvelope::ok(
            GALLERY_FILTER_API_VERSION,
            GALLERY_FILTER_MODULE,
            GalleryFilterPlan {
                operation: GalleryFilterOperation::Comment,
                keep: vec![true, true],
                needs_tags: false,
                comment_evaluation_filter_indices: vec![],
            },
        );

        assert_eq!(
            encode_hex(&post_request),
            "a76776657273696f6e01696f7065726174696f6e6e706f7374456e726963686d656e746766696c7465727381a3646d6f64650264746578746a66656d616c653a74616767656e61626c6564f56967616c6c657269657381a3657469746c65f66875706c6f61646572f66474616773826a66656d616c653a7461676b67726f75703a76616c756568636f6d6d656e7473806e73636f72655468726573686f6c64f675636f6d6d656e74436f6e74656e744d617463686573f6"
        );
        assert_eq!(
            encode_hex(&post_envelope),
            "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e6e706f7374456e726963686d656e74646b65657081f4696e6565647354616773f5781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
        );
        assert_eq!(
            encode_hex(&comment_preflight_request),
            "a76776657273696f6e01696f7065726174696f6e67636f6d6d656e746766696c7465727382a3646d6f646504647465787465616c69636567656e61626c6564f5a3646d6f6465056474657874677061747465726e67656e61626c6564f56967616c6c65726965738068636f6d6d656e747382a36573636f7265016875706c6f61646572f569636f6d6d656e74657265616c696365a36573636f7265146875706c6f61646572f469636f6d6d656e74657263626f626e73636f72655468726573686f6c640a75636f6d6d656e74436f6e74656e744d617463686573f6"
        );
        assert_eq!(
            encode_hex(&comment_preflight_envelope),
            "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e67636f6d6d656e74646b65657080696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657382808101656572726f72f6"
        );
        assert_eq!(
            encode_hex(&comment_final_request),
            "a76776657273696f6e01696f7065726174696f6e67636f6d6d656e746766696c7465727382a3646d6f646504647465787465616c69636567656e61626c6564f5a3646d6f6465056474657874677061747465726e67656e61626c6564f56967616c6c65726965738068636f6d6d656e747382a36573636f7265016875706c6f61646572f569636f6d6d656e74657265616c696365a36573636f7265146875706c6f61646572f469636f6d6d656e74657263626f626e73636f72655468726573686f6c640a75636f6d6d656e74436f6e74656e744d6174636865738282f6f682f6f4"
        );
        assert_eq!(
            encode_hex(&comment_final_envelope),
            "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e67636f6d6d656e74646b65657082f5f5696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
        );
    }

    fn filter(mode: i32, text: &str, enabled: bool) -> GalleryFilterSnapshot {
        GalleryFilterSnapshot {
            mode,
            text: text.to_owned(),
            enabled,
        }
    }

    fn gallery(
        title: Option<&str>,
        uploader: Option<&str>,
        tags: Option<Vec<&str>>,
    ) -> GalleryFilterGallery {
        GalleryFilterGallery {
            title: title.map(str::to_owned),
            uploader: uploader.map(str::to_owned),
            tags: tags.map(|tags| tags.into_iter().map(str::to_owned).collect()),
        }
    }

    fn comment(score: i32, uploader: bool, commenter: Option<&str>) -> GalleryFilterComment {
        GalleryFilterComment {
            score,
            uploader,
            commenter: commenter.map(str::to_owned),
        }
    }

    fn list_request(
        operation: GalleryFilterOperation,
        filters: Vec<GalleryFilterSnapshot>,
        galleries: Vec<GalleryFilterGallery>,
    ) -> GalleryFilterRequest {
        GalleryFilterRequest {
            version: GALLERY_FILTER_API_VERSION,
            operation,
            filters,
            galleries,
            comments: vec![],
            score_threshold: None,
            comment_content_matches: None,
        }
    }

    fn comment_request(
        filters: Vec<GalleryFilterSnapshot>,
        comments: Vec<GalleryFilterComment>,
        score_threshold: i32,
        comment_content_matches: Option<Vec<Vec<Option<bool>>>>,
    ) -> GalleryFilterRequest {
        GalleryFilterRequest {
            version: GALLERY_FILTER_API_VERSION,
            operation: GalleryFilterOperation::Comment,
            filters,
            galleries: vec![],
            comments,
            score_threshold: Some(score_threshold),
            comment_content_matches,
        }
    }

    fn assert_error(request: &GalleryFilterRequest, expected: &str) {
        assert_eq!(
            plan_gallery_filter(request)
                .expect_err("request must be rejected")
                .to_string(),
            expected
        );
    }

    fn encode_hex<T: Serialize>(value: &T) -> String {
        let bytes = serde_cbor::to_vec(value).expect("fixture must encode");
        bytes.iter().fold(
            String::with_capacity(bytes.len() * 2),
            |mut encoded, byte| {
                write!(&mut encoded, "{byte:02x}").expect("String writes cannot fail");
                encoded
            },
        )
    }
}
