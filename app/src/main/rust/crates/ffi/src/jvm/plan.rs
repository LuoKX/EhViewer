//! Request/plan JNI exports backed by versioned domain and network contracts.

use anyhow::ensure;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::jint;
use jni_fn::jni_fn;

use eh_domain::contracts::ApiEnvelope;
use eh_domain::daily::{
    DAILY_CHECK_API_VERSION, DAILY_CHECK_MODULE, DailyCheckRequest, plan_daily_check,
};
use eh_domain::download::progress::{
    DOWNLOAD_PROGRESS_API_VERSION, DOWNLOAD_PROGRESS_MODULE, DownloadProgressPlan,
    DownloadProgressRequest, plan_download_progress,
};
use eh_domain::download::{
    DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, DownloadQueueRecoveryRequest,
    DownloadQueueRequest, recover_download_queue, reduce_download_queue,
};
use eh_domain::download_catalog::{
    DOWNLOAD_CATALOG_API_VERSION, DOWNLOAD_CATALOG_MODULE, DownloadFilterRequest,
    DownloadSortRequest, filter_download_catalog, sort_download_catalog,
};
use eh_domain::library::{
    DatabaseSearchQueryRequest, LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE,
    LocalFavoriteMutationRequest, QuickSearchImportRequest, QuickSearchMatchRequest,
    plan_database_search_query, plan_local_favorite_mutation, plan_quick_search_import,
    plan_quick_search_match,
};
use eh_domain::reader::fetch::{
    READER_FETCH_API_VERSION, READER_FETCH_MODULE, ReaderFetchPlan, ReaderFetchRequest,
    plan_reader_fetch,
};
use eh_domain::reader::{
    READER_SCHEDULE_API_VERSION, READER_SCHEDULE_MODULE, ReaderScheduleRequest,
    plan_reader_schedule,
};
use eh_domain::search::{
    SEARCH_RULE_API_VERSION, SEARCH_RULE_MODULE, SearchRuleRequest, plan_search_rule,
};
use eh_domain::session::lifecycle::{
    SESSION_LIFECYCLE_API_VERSION, SESSION_LIFECYCLE_MODULE, SessionLifecycleRequest,
    plan_session_lifecycle,
};
use eh_domain::session::{
    SESSION_POLICY_API_VERSION, SESSION_POLICY_MODULE, SessionFailureRequest,
    classify_session_failure,
};
use eh_net::gallery::archive_request::{
    API_VERSION as ARCHIVE_REQUEST_API_VERSION, GalleryArchiveRequest,
    build_envelope as build_gallery_archive_request_envelope,
};
use eh_net::gallery::favorite_mutation::{
    API_VERSION as FAVORITE_MUTATION_API_VERSION, FavoriteMutationRequest,
    build_envelope as build_favorite_mutation_envelope,
};
use eh_net::gallery::favorites_url::{
    API_VERSION as FAVORITES_URL_API_VERSION, FavoritesUrlRequest,
    build_envelope as build_favorites_url_envelope,
};
use eh_net::gallery::list_url::{
    API_VERSION as GALLERY_LIST_URL_API_VERSION, GalleryListUrlRequest,
    build_envelope as build_gallery_list_url_envelope,
};
use eh_net::gallery::list_url_parser::{
    API_VERSION as GALLERY_LIST_URL_PARSER_API_VERSION, GalleryListUrlParseRequest,
    parse_envelope as parse_gallery_list_url_envelope,
};
use eh_net::gallery::navigation::{
    API_VERSION as GALLERY_NAVIGATION_API_VERSION, GalleryNavigationRequest,
    parse_envelope as parse_gallery_navigation_envelope,
};

use super::{
    decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer, jni_throwing,
    serialize_to_buffer,
};

fn plan_download_progress_envelope(
    request: DownloadProgressRequest,
) -> anyhow::Result<ApiEnvelope<DownloadProgressPlan>> {
    ensure!(
        request.version == DOWNLOAD_PROGRESS_API_VERSION,
        "Unsupported Download Progress API version: {}",
        request.version
    );
    Ok(ApiEnvelope::ok(
        DOWNLOAD_PROGRESS_API_VERSION,
        DOWNLOAD_PROGRESS_MODULE,
        plan_download_progress(request.snapshot, request.command),
    ))
}

fn plan_reader_fetch_envelope(
    request: ReaderFetchRequest,
) -> anyhow::Result<ApiEnvelope<ReaderFetchPlan>> {
    ensure!(
        request.version == READER_FETCH_API_VERSION,
        "Unsupported Reader Fetch API version: {}",
        request.version
    );
    let plan = plan_reader_fetch(request.state, request.event)?;
    Ok(ApiEnvelope::ok(
        READER_FETCH_API_VERSION,
        READER_FETCH_MODULE,
        plan,
    ))
}

/// Plans runtime download counters and ETA through the pure domain progress module.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadProgressBridgeKt")]
pub fn nativePlanDownloadProgressV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DownloadProgressRequest = decode_request(request_bytes, request_length)?;
        let response = plan_download_progress_envelope(request)?;
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Inspects one bounded local download-recovery candidate through `eh_domain`.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadRecoveryBridgeKt")]
pub fn nativeInspectDownloadRecoveryCandidateV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: eh_domain::recovery::RecoveryCandidateRequest =
            decode_request(request_bytes, request_length)?;
        let response = eh_domain::recovery::inspect_recovery_candidate_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

fn decode_download_recovery_conflict_request(
    buffer: &[u8],
    limit: jint,
) -> anyhow::Result<eh_domain::recovery::RecoveryConflictRequest> {
    let input = crate::codec::input_bytes(buffer, limit)?;
    serde_cbor::from_slice(input).map_err(|error| {
        let category = if error.is_data() {
            "data"
        } else if error.is_syntax() {
            "syntax"
        } else if error.is_eof() {
            "eof"
        } else {
            "io"
        };
        anyhow::anyhow!(
            "Download recovery CBOR {category} error at byte {}; detail: {}",
            error.offset(),
            sanitize_download_recovery_cbor_detail(&error)
        )
    })
}

fn sanitize_download_recovery_cbor_detail(error: &serde_cbor::Error) -> String {
    const SAFE_SCHEMA_IDENTIFIERS: &[&str] = &[
        "version",
        "candidates",
        "existing",
        "sourceKey",
        "fingerprint",
        "sourceKind",
        "displayName",
        "gid",
        "token",
        "title",
        "declaredPages",
        "localPageCount",
        "evidence",
        "issues",
        "status",
        "action",
        "kind",
        "observedPages",
        "complete",
    ];
    const REDACTED: &str = "<redacted>";
    const NUMBER: &str = "<number>";
    const MAX_DETAIL_CHARS: usize = 256;

    let raw = error.to_string();
    let mut chars = raw.chars().peekable();
    let mut sanitized = String::with_capacity(raw.len().min(MAX_DETAIL_CHARS));
    let mut previous_is_identifier = false;
    while let Some(character) = chars.next() {
        if matches!(character, '\'' | '"' | '`') {
            let quote = character;
            let mut quoted = String::new();
            for next in chars.by_ref() {
                if next == quote {
                    break;
                }
                quoted.push(next);
            }
            if SAFE_SCHEMA_IDENTIFIERS.contains(&quoted.as_str()) {
                sanitized.push(quote);
                sanitized.push_str(&quoted);
                sanitized.push(quote);
            } else {
                sanitized.push_str(REDACTED);
            }
            previous_is_identifier = false;
        } else if character.is_ascii_digit() && !previous_is_identifier {
            while chars.peek().is_some_and(char::is_ascii_digit) {
                chars.next();
            }
            sanitized.push_str(NUMBER);
            previous_is_identifier = false;
        } else {
            sanitized.push(character);
            previous_is_identifier = character.is_ascii_alphabetic() || character == '_';
        }
        if sanitized.chars().count() >= MAX_DETAIL_CHARS {
            sanitized.truncate(MAX_DETAIL_CHARS);
            break;
        }
    }
    sanitized
}

/// Reconciles a bounded recovery batch against current Android-owned mappings.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadRecoveryBridgeKt")]
pub fn nativePlanDownloadRecoveryConflictsV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: eh_domain::recovery::RecoveryConflictRequest =
            decode_download_recovery_conflict_request(request_bytes, request_length)?;
        let response = eh_domain::recovery::plan_recovery_conflicts_envelope(request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Reduces one versioned Android download-queue snapshot through `eh_domain`.
///
/// Request and response use separate direct buffers so serialization is atomic.
/// Invalid CBOR, unsupported versions, invalid queue invariants, and insufficient
/// output capacity become contained Java `RuntimeException`s.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadQueueBridgeKt")]
pub fn nativeReduceDownloadQueueV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DownloadQueueRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == DOWNLOAD_QUEUE_API_VERSION,
            "Unsupported Download Queue API version: {}",
            request.version
        );
        let plan = reduce_download_queue(request.snapshot, request.command)?;
        let response = ApiEnvelope::ok(DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Normalizes persisted Android queue entries after process death.
///
/// The recovered plan always has an empty FIFO and no active worker. Request and
/// response buffers are separate so a failed decode or serialization cannot
/// corrupt the caller's request.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadQueueBridgeKt")]
pub fn nativeRecoverDownloadQueueV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DownloadQueueRecoveryRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == DOWNLOAD_QUEUE_API_VERSION,
            "Unsupported Download Queue API version: {}",
            request.version
        );
        let plan = recover_download_queue(request.entries)?;
        let response = ApiEnvelope::ok(DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one sanitized reader-image fetch transition through `eh_domain`.
///
/// Rust returns only the next state and ordered platform-neutral effects. Android
/// retains cache and file access, network requests, tokens, URLs, display errors,
/// listeners, and coroutine cancellation.
#[jni_fn("com.hippo.ehviewer.bridge.reader.ReaderFetchBridgeKt")]
pub fn nativePlanReaderFetchV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: ReaderFetchRequest = decode_request(request_bytes, request_length)?;
        let response = plan_reader_fetch_envelope(request)?;
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one versioned Android reader command through `eh_domain`.
///
/// Rust returns only ordered platform-neutral actions. Android retains ownership
/// of image objects, page flows, caches, network requests, and archive handles.
#[jni_fn("com.hippo.ehviewer.bridge.reader.ReaderScheduleBridgeKt")]
pub fn nativePlanReaderScheduleV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: ReaderScheduleRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == READER_SCHEDULE_API_VERSION,
            "Unsupported Reader Schedule API version: {}",
            request.version
        );
        let plan = plan_reader_schedule(&request.snapshot, request.command)?;
        let response = ApiEnvelope::ok(READER_SCHEDULE_API_VERSION, READER_SCHEDULE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Classifies one sanitized failed HTTP response through `eh_domain`.
///
/// Android retains cookies, credentials, URLs, response bodies, localized text,
/// parse-error files, HTTP clients, and exception construction.
#[jni_fn("com.hippo.ehviewer.bridge.auth.SessionPolicyBridgeKt")]
pub fn nativeClassifySessionFailureV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: SessionFailureRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == SESSION_POLICY_API_VERSION,
            "Unsupported Session Policy API version: {}",
            request.version
        );
        let plan = classify_session_failure(&request.snapshot);
        let response = ApiEnvelope::ok(SESSION_POLICY_API_VERSION, SESSION_POLICY_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one sanitized startup, authentication, guest, or logout transition.
///
/// Android retains `CookieManager`, credentials, HTTP, `WebView`, Settings, and all
/// platform effects. A missing identity-cookie pair is returned as typed
/// `NeedLogin`; transport and version failures become contained exceptions.
#[jni_fn("com.hippo.ehviewer.bridge.auth.SessionLifecycleBridgeKt")]
pub fn nativePlanSessionLifecycleV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: SessionLifecycleRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == SESSION_LIFECYCLE_API_VERSION,
            "Unsupported Session Lifecycle API version: {}",
            request.version
        );
        let response = match plan_session_lifecycle(&request.snapshot, request.command) {
            Ok(plan) => ApiEnvelope::ok(
                SESSION_LIFECYCLE_API_VERSION,
                SESSION_LIFECYCLE_MODULE,
                plan,
            ),
            Err(error) => ApiEnvelope::err(
                SESSION_LIFECYCLE_API_VERSION,
                SESSION_LIFECYCLE_MODULE,
                error,
            ),
        };
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one sanitized daily event-check eligibility decision.
///
/// Android retains the platform clock, `WorkManager`, authentication state,
/// persistence, HTTP, and notifications.
#[jni_fn("com.hippo.ehviewer.bridge.DailyCheckBridgeKt")]
pub fn nativePlanDailyCheckV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DailyCheckRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == DAILY_CHECK_API_VERSION,
            "Unsupported Daily Check API version: {}",
            request.version
        );
        let plan = plan_daily_check(&request);
        let response = ApiEnvelope::ok(DAILY_CHECK_API_VERSION, DAILY_CHECK_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Applies one sanitized query-normalization or tag-formatting rule.
///
/// Android retains text fields, history persistence, suggestions, and the tag
/// translation database.
#[jni_fn("com.hippo.ehviewer.bridge.SearchRuleBridgeKt")]
pub fn nativePlanSearchRuleV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: SearchRuleRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == SEARCH_RULE_API_VERSION,
            "Unsupported Search Rule API version: {}",
            request.version
        );
        ensure!(
            request.has_valid_shape(),
            "Invalid Search Rule request shape"
        );
        let plan = plan_search_rule(&request);
        let response = ApiEnvelope::ok(SEARCH_RULE_API_VERSION, SEARCH_RULE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Sorts one immutable Android download-catalog snapshot.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadCatalogBridgeKt")]
pub fn nativeSortDownloadCatalogV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DownloadSortRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == DOWNLOAD_CATALOG_API_VERSION,
            "Unsupported Download Catalog API version: {}",
            request.version
        );
        let plan = sort_download_catalog(&request);
        let response = ApiEnvelope::ok(DOWNLOAD_CATALOG_API_VERSION, DOWNLOAD_CATALOG_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Filters one immutable Android download-catalog snapshot without reordering it.
#[jni_fn("com.hippo.ehviewer.bridge.download.DownloadCatalogBridgeKt")]
pub fn nativeFilterDownloadCatalogV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DownloadFilterRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == DOWNLOAD_CATALOG_API_VERSION,
            "Unsupported Download Catalog API version: {}",
            request.version
        );
        let plan = filter_download_catalog(&request);
        let response = ApiEnvelope::ok(DOWNLOAD_CATALOG_API_VERSION, DOWNLOAD_CATALOG_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Shapes one Android-owned Room FTS query.
#[jni_fn("com.hippo.ehviewer.bridge.LibraryRuleBridgeKt")]
pub fn nativePlanDatabaseSearchQueryV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: DatabaseSearchQueryRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == LIBRARY_RULE_API_VERSION,
            "Unsupported Library Rule API version"
        );
        let plan = plan_database_search_query(&request);
        let response = ApiEnvelope::ok(LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Compares one mutable builder snapshot with an optional Android Room row.
#[jni_fn("com.hippo.ehviewer.bridge.LibraryRuleBridgeKt")]
pub fn nativePlanQuickSearchMatchV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: QuickSearchMatchRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == LIBRARY_RULE_API_VERSION,
            "Unsupported Library Rule API version"
        );
        let plan = plan_quick_search_match(&request);
        let response = ApiEnvelope::ok(LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans an additive `QuickSearch` import before Android performs its Room insert.
#[jni_fn("com.hippo.ehviewer.bridge.QuickSearchImportBridgeKt")]
pub fn nativePlanQuickSearchImportV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: QuickSearchImportRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == LIBRARY_RULE_API_VERSION,
            "Unsupported Library Rule API version"
        );
        let plan = plan_quick_search_import(&request);
        let response = ApiEnvelope::ok(LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE, plan);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one local/cloud favorite transition before Android performs effects.
#[jni_fn("com.hippo.ehviewer.bridge.LibraryRuleBridgeKt")]
pub fn nativePlanLocalFavoriteMutationV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: LocalFavoriteMutationRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == LIBRARY_RULE_API_VERSION,
            "Unsupported Library Rule API version"
        );
        let response = match plan_local_favorite_mutation(request) {
            Ok(plan) => ApiEnvelope::ok(LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE, plan),
            Err(error) => ApiEnvelope::err(LIBRARY_RULE_API_VERSION, LIBRARY_RULE_MODULE, error),
        };
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Constructs one sanitized Gallery List request URL through `eh_net`.
///
/// Android retains mutable navigation state and site selection.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryListUrlBridgeKt")]
pub fn nativeBuildGalleryListUrlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryListUrlRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == GALLERY_LIST_URL_API_VERSION,
            "Unsupported Gallery List URL API version: {}",
            request.version
        );
        let response = build_gallery_list_url_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Interprets sanitized Gallery List URL components through `eh_net`.
///
/// Android retains Ktor syntax parsing, the exact host gate, and mutable navigation state.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryListUrlParserBridgeKt")]
pub fn nativeParseGalleryListUrlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryListUrlParseRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == GALLERY_LIST_URL_PARSER_API_VERSION,
            "Unsupported Gallery List URL Parser API version: {}",
            request.version
        );
        let response = parse_gallery_list_url_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Parses one gallery detail or reader-page navigation reference through `eh_net`.
///
/// Android retains navigation and destination construction. Candidate URLs and
/// tokens are never attached to boundary errors, and no parser fallback is linked.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryNavigationBridgeKt")]
pub fn nativeParseGalleryNavigationV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryNavigationRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == GALLERY_NAVIGATION_API_VERSION,
            "Unsupported Gallery Navigation API version: {}",
            request.version
        );
        let response = parse_gallery_navigation_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Constructs one sanitized remote Favorites request URL through `eh_net`.
///
/// Android retains local favorites, mutable navigation state, site selection, and rollback.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.FavoritesUrlBridgeKt")]
pub fn nativeBuildFavoritesUrlV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: FavoritesUrlRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == FAVORITES_URL_API_VERSION,
            "Unsupported Favorites URL API version: {}",
            request.version
        );
        let response = build_favorites_url_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one pure Gallery Favorites mutation request; Android owns HTTP and effects.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.FavoriteMutationBridgeKt")]
pub fn nativePlanFavoriteMutationV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: FavoriteMutationRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == FAVORITE_MUTATION_API_VERSION,
            "Unsupported Gallery Favorites mutation API version: {}",
            request.version
        );
        let response = build_favorite_mutation_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

/// Plans one pure Gallery Archive request; Android owns HTTP and effects.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryArchiveRequestBridgeKt")]
pub fn nativePlanGalleryArchiveRequestV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: GalleryArchiveRequest = decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == ARCHIVE_REQUEST_API_VERSION,
            "Unsupported Gallery Archive request API version: {}",
            request.version
        );
        let response = build_gallery_archive_request_envelope(&request);
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

#[jni_fn("com.hippo.ehviewer.bridge.SettingsRuleBridgeKt")]
pub fn nativePlanSettingsRepairV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: eh_domain::settings::SettingsRepairRequest =
            decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == eh_domain::settings::SETTINGS_RULE_API_VERSION,
            "Unsupported Settings Rule API version"
        );
        let plan = eh_domain::settings::plan_settings_repair(&request);
        let response = ApiEnvelope::ok(
            eh_domain::settings::SETTINGS_RULE_API_VERSION,
            eh_domain::settings::SETTINGS_RULE_MODULE,
            plan,
        );
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

#[jni_fn("com.hippo.ehviewer.bridge.SettingsRuleBridgeKt")]
pub fn nativePlanReadCacheSizeV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
    response_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let request_bytes = deref_direct_bytebuffer(env, &request_buffer)?;
        let request: eh_domain::settings::ReadCacheSizeRequest =
            decode_request(request_bytes, request_length)?;
        ensure!(
            request.version == eh_domain::settings::SETTINGS_RULE_API_VERSION,
            "Unsupported Settings Rule API version"
        );
        let plan = eh_domain::settings::plan_read_cache_size(&request);
        let response = ApiEnvelope::ok(
            eh_domain::settings::SETTINGS_RULE_API_VERSION,
            eh_domain::settings::SETTINGS_RULE_MODULE,
            plan,
        );
        let response_bytes = deref_mut_direct_bytebuffer(env, &response_buffer)?;
        serialize_to_buffer(response_bytes, &response)
    })
}

#[cfg(test)]
mod tests {
    use eh_domain::download::DownloadState;
    use eh_domain::download::progress::{DownloadProgressCommand, DownloadProgressSnapshot};
    use eh_domain::reader::fetch::{ReaderFetchEvent, ReaderFetchPhase, ReaderFetchState};

    use super::*;

    fn progress_request(version: u8) -> DownloadProgressRequest {
        DownloadProgressRequest {
            version,
            snapshot: DownloadProgressSnapshot {
                state: DownloadState::None,
                speed_bytes_per_second: -1,
                remaining_milliseconds: -1,
                finished: 0,
                downloaded: 0,
                total: -1,
                legacy: -1,
            },
            command: DownloadProgressCommand::Reset,
        }
    }

    fn reader_fetch_request(version: u8) -> ReaderFetchRequest {
        ReaderFetchRequest {
            version,
            state: ReaderFetchState::initial(false, false, true),
            event: ReaderFetchEvent::Start,
        }
    }

    #[test]
    fn download_progress_ffi_plan_builds_the_exact_v1_envelope() {
        let envelope = plan_download_progress_envelope(progress_request(1))
            .expect("supported progress version should plan");

        assert_eq!(envelope.version, DOWNLOAD_PROGRESS_API_VERSION);
        assert_eq!(envelope.module, DOWNLOAD_PROGRESS_MODULE);
        assert!(envelope.error.is_none());
        let payload = envelope.payload.expect("success payload");
        assert_eq!(payload.snapshot.state, DownloadState::Downloading);
    }

    #[test]
    fn download_progress_ffi_plan_rejects_unsupported_version() {
        let error = plan_download_progress_envelope(progress_request(2))
            .expect_err("unsupported progress version must fail before planning");

        assert_eq!(
            error.to_string(),
            "Unsupported Download Progress API version: 2"
        );
    }

    #[test]
    fn reader_fetch_ffi_plan_builds_the_exact_v1_envelope() {
        let envelope = plan_reader_fetch_envelope(reader_fetch_request(1))
            .expect("supported reader fetch version should plan");

        assert_eq!(envelope.version, READER_FETCH_API_VERSION);
        assert_eq!(envelope.module, READER_FETCH_MODULE);
        assert!(envelope.error.is_none());
        let payload = envelope.payload.expect("success payload");
        assert_eq!(payload.state.phase, ReaderFetchPhase::CheckingCache);
    }

    #[test]
    fn reader_fetch_ffi_plan_rejects_unsupported_version() {
        let error = plan_reader_fetch_envelope(reader_fetch_request(2))
            .expect_err("unsupported reader fetch version must fail before planning");

        assert_eq!(error.to_string(), "Unsupported Reader Fetch API version: 2");
    }
}
