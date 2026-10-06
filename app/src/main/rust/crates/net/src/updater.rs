//! Bounded GitHub response protocol composed with pure update policy; no HTTP or authentication.
use eh_domain::contracts::{ApiEnvelope, RustError};
use eh_domain::update_policy::{
    self, MAX_ITEMS, MAX_TEXT, UpdateOperation, UpdatePolicyPlan, UpdatePolicyRequest,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Maximum UTF-8 JSON bytes (including ignored fields and nesting).
pub const MAX_JSON: usize = 2 * 1024 * 1024;
/// Maximum encoded JNI request bytes.
pub const MAX_REQUEST: usize = MAX_JSON + 1024 * 1024;
/// Fixed protocol error, without body/URL/auth details.
pub const INVALID_RESPONSE: &str = "Invalid update response";

/// Closed remote-response stages.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateResponseOperation {
    /// Helper or eligibility facts; no JSON.
    Policy,
    /// Repository default branch.
    Repo,
    /// Release selection and comparison in one call.
    Release,
    /// First workflow selection and short-SHA comparison in one call.
    Workflow,
    /// Sorted artifact selection in one call.
    Artifacts,
    /// Decode commit facts; presentation formatting remains Kotlin.
    Comparison,
}
/// V1 transport request. Credentials and request headers are never fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRequest {
    /// Exact envelope version.
    pub version: u8,
    /// Response stage.
    pub operation: UpdateResponseOperation,
    /// Immutable build/clock/selection facts.
    pub policy: UpdatePolicyRequest,
    /// Already received GitHub JSON, absent for helper/clock plans.
    pub json: Option<String>,
}
/// First workflow response fact, preserving original field names for Android models.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkflowRun {
    /// Full server SHA.
    pub head_sha: String,
    /// Fallback display title.
    pub display_title: String,
    /// Artifact endpoint, not fetched by Rust.
    pub artifacts_url: String,
}
/// Presentation facts only, with original GitHub JSON names.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Commit {
    /// Commit detail.
    pub commit: CommitDetail,
}
/// Commit display facts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitDetail {
    /// Author display name.
    pub author: CommitAuthor,
    /// Full message, formatted later by Android.
    pub message: String,
}
/// Commit author fact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitAuthor {
    /// Display name, never an authentication identity.
    pub name: String,
}
/// Complete response plan. Exactly one stage-specific shape is returned.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePlan {
    /// Pure policy result, absent for repository/comparison decoding.
    pub policy: Option<UpdatePolicyPlan>,
    /// Server branch, present only for repository responses.
    pub branch: Option<String>,
    /// Selected first run, present only for workflow responses.
    pub workflow_run: Option<WorkflowRun>,
    /// Release body, present only for release responses.
    pub info: Option<String>,
    /// Selected URL, present only for release/artifact responses.
    pub download_link: Option<String>,
    /// Selected original server index for remote response selection.
    pub selected_index: Option<usize>,
    /// Original server collection size to bound source index.
    pub source_count: Option<usize>,
    /// Commit display facts, present only for comparison responses.
    pub commits: Option<Vec<Commit>>,
}
#[derive(Deserialize)]
struct Repo {
    default_branch: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    body: String,
    html_url: String,
    assets: Vec<Asset>,
}
#[derive(Deserialize)]
struct Asset {
    url: String,
    name: String,
}
#[derive(Deserialize)]
struct Runs {
    workflow_runs: Vec<WorkflowRun>,
}
#[derive(Deserialize)]
struct Artifacts {
    artifacts: Vec<Artifact>,
}
#[derive(Deserialize)]
struct Artifact {
    name: String,
    archive_download_url: String,
}
#[derive(Deserialize)]
struct Comparison {
    commits: Vec<Commit>,
}

/// Compose bounded protocol decoding with whole-response validation and selection.
#[must_use]
pub fn build_envelope(request: &UpdateRequest) -> ApiEnvelope<UpdatePlan> {
    match build_plan(request) {
        Ok(plan) => ApiEnvelope::ok(1, "updater", plan),
        Err(error) => ApiEnvelope::err(1, "updater", error),
    }
}
fn build_plan(request: &UpdateRequest) -> Result<UpdatePlan, RustError> {
    if request.version != 1
        || (request.operation == UpdateResponseOperation::Policy) != request.json.is_none()
    {
        return Err(update_policy::error(update_policy::INVALID_INPUT));
    }
    // Validate every scalar fact before parsing any remote JSON.
    update_policy::plan(&request.policy)?;
    let mut result = UpdatePlan {
        policy: None,
        branch: None,
        workflow_run: None,
        info: None,
        download_link: None,
        selected_index: None,
        source_count: None,
        commits: None,
    };
    match request.operation {
        UpdateResponseOperation::Policy => {
            result.policy = Some(update_policy::plan(&request.policy)?);
        }
        UpdateResponseOperation::Repo => {
            let repo: Repo = parse(request)?;
            text(&repo.default_branch)?;
            result.branch = Some(repo.default_branch);
        }
        UpdateResponseOperation::Release => {
            let release: Release = parse(request)?;
            count(release.assets.len())?;
            for value in [&release.tag_name, &release.body, &release.html_url]
                .into_iter()
                .chain(
                    release
                        .assets
                        .iter()
                        .flat_map(|asset| [&asset.name, &asset.url]),
                )
            {
                text(value)?;
            }
            let selected = select(
                request,
                UpdateOperation::SelectRelease,
                release
                    .assets
                    .iter()
                    .map(|asset| asset.name.clone())
                    .collect(),
            )?;
            let index = selected.index.expect("selection index");
            result.source_count = Some(release.assets.len());
            result.selected_index = Some(index);
            result.download_link = Some(release.assets[index].url.clone());
            result.info = Some(release.body);
            result.policy = Some(compare(
                request,
                UpdateOperation::CompareRelease,
                release.tag_name,
            )?);
        }
        UpdateResponseOperation::Workflow => workflow(request, &mut result)?,
        UpdateResponseOperation::Artifacts => {
            let artifacts: Artifacts = parse(request)?;
            count(artifacts.artifacts.len())?;
            for artifact in &artifacts.artifacts {
                text(&artifact.name)?;
                text(&artifact.archive_download_url)?;
            }
            // Expired/unknown fields remain ignored, exactly like the Kotlin model.
            let selected = select(
                request,
                UpdateOperation::SelectArtifacts,
                artifacts
                    .artifacts
                    .iter()
                    .map(|asset| asset.name.clone())
                    .collect(),
            )?;
            let index = selected.index.expect("selection index");
            result.source_count = Some(artifacts.artifacts.len());
            result.selected_index = Some(index);
            result.download_link = Some(artifacts.artifacts[index].archive_download_url.clone());
            result.policy = Some(selected);
        }
        UpdateResponseOperation::Comparison => {
            let comparison: Comparison = parse(request)?;
            count(comparison.commits.len())?;
            for commit in &comparison.commits {
                text(&commit.commit.message)?;
                text(&commit.commit.author.name)?;
            }
            result.commits = Some(comparison.commits);
        }
    }
    Ok(result)
}
fn workflow(request: &UpdateRequest, result: &mut UpdatePlan) -> Result<(), RustError> {
    let runs: Runs = parse(request)?;
    count(runs.workflow_runs.len())?;
    for run in &runs.workflow_runs {
        text(&run.head_sha)?;
        text(&run.display_title)?;
        text(&run.artifacts_url)?;
    }
    let selected = select(
        request,
        UpdateOperation::SelectWorkflow,
        runs.workflow_runs
            .iter()
            .map(|run| run.head_sha.clone())
            .collect(),
    )?;
    result.source_count = Some(runs.workflow_runs.len());
    result.selected_index = selected.index;
    let run = runs
        .workflow_runs
        .into_iter()
        .next()
        .expect("nonempty selection");
    result.policy = Some(compare(
        request,
        UpdateOperation::CompareWorkflow,
        run.head_sha.clone(),
    )?);
    result.workflow_run = Some(run);
    Ok(())
}
fn parse<T: DeserializeOwned>(request: &UpdateRequest) -> Result<T, RustError> {
    let json = request
        .json
        .as_ref()
        .ok_or_else(|| update_policy::error(INVALID_RESPONSE))?;
    if json.len() > MAX_JSON {
        return Err(update_policy::error(INVALID_RESPONSE));
    }
    // Value preserves Kotlin's last-duplicate-field wins, including ignored fields.
    // serde_json's default recursion limit bounds depth; raw byte budget bounds allocation.
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|_| update_policy::error(INVALID_RESPONSE))?;
    serde_json::from_value(value).map_err(|_| update_policy::error(INVALID_RESPONSE))
}
fn select(
    request: &UpdateRequest,
    operation: UpdateOperation,
    names: Vec<String>,
) -> Result<UpdatePolicyPlan, RustError> {
    let mut policy = request.policy.clone();
    policy.operation = operation;
    policy.names = names;
    policy.matches = None;
    update_policy::plan(&policy)
}
fn compare(
    request: &UpdateRequest,
    operation: UpdateOperation,
    latest: String,
) -> Result<UpdatePolicyPlan, RustError> {
    let mut policy = request.policy.clone();
    policy.operation = operation;
    policy.latest = latest;
    update_policy::plan(&policy)
}
fn text(value: &str) -> Result<(), RustError> {
    if value.len() > MAX_TEXT {
        Err(update_policy::error(INVALID_RESPONSE))
    } else {
        Ok(())
    }
}
fn count(size: usize) -> Result<(), RustError> {
    if size > MAX_ITEMS {
        Err(update_policy::error(INVALID_RESPONSE))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
