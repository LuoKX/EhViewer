package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** Closed phases owned by `eh_domain::reader::fetch::ReaderFetchPhase`. */
@Serializable
internal enum class ReaderFetchPhase {
  @SerialName("initial") INITIAL,
  @SerialName("checkingCache") CHECKING_CACHE,
  @SerialName("acquiringTokens") ACQUIRING_TOKENS,
  @SerialName("fetchingPage") FETCHING_PAGE,
  @SerialName("resolvingOriginal") RESOLVING_ORIGINAL,
  @SerialName("downloadingImage") DOWNLOADING_IMAGE,
  @SerialName("classifyingOuterFailure") CLASSIFYING_OUTER_FAILURE,
  @SerialName("finished") FINISHED,
  @SerialName("failed") FAILED,
  @SerialName("cancelled") CANCELLED,
}

/** Closed page-metadata sources selected by the reader fetch reducer. */
@Serializable
internal enum class ReaderFetchPageSource {
  @SerialName("html") HTML,
  @SerialName("api") API,
}

/** Closed image targets emitted by the reader fetch reducer. */
@Serializable
internal enum class ReaderFetchImageTarget {
  @SerialName("original") ORIGINAL,
  @SerialName("resized") RESIZED,
}

/** Sanitized image-download failure classifications supplied by Android. */
@Serializable
internal enum class ReaderFetchImageFailure {
  @SerialName("recoverable") RECOVERABLE,
  @SerialName("fileNotFound") FILE_NOT_FOUND,
  @SerialName("cancellation") CANCELLATION,
}

/** Android-owned display-error source selected by the reader fetch reducer. */
@Serializable
internal enum class ReaderFetchErrorSource {
  @SerialName("pTokenMessage") P_TOKEN_MESSAGE,
  @SerialName("retained") RETAINED,
}

/** Complete sanitized state for one reader-image fetch operation. */
@Serializable
internal data class ReaderFetchState(
  @SerialName("phase") val phase: ReaderFetchPhase,
  @SerialName("attempt") val attempt: Int,
  @SerialName("forceHtml") val forceHtml: Boolean,
  @SerialName("force") val force: Boolean,
  @SerialName("skipHath") val skipHath: Boolean,
  @SerialName("originalRequested") val originalRequested: Boolean,
  @SerialName("showKeyAvailable") val showKeyAvailable: Boolean,
  @SerialName("pageSource") val pageSource: ReaderFetchPageSource?,
) {
  init {
    require(attempt in 0..MAX_READER_FETCH_ATTEMPT) {
      "Reader Fetch attempt is outside the supported range"
    }
    require(!skipHath || force) { "Reader Fetch skip-H@H requires a forced request" }
    require((phase == ReaderFetchPhase.FETCHING_PAGE) == (pageSource != null)) {
      "Reader Fetch page source does not match the phase"
    }
    require(phase != ReaderFetchPhase.CHECKING_CACHE || !force) {
      "Reader Fetch forced requests cannot check the cache"
    }
    require(
      phase != ReaderFetchPhase.INITIAL ||
        (attempt == 0 && !forceHtml && !showKeyAvailable && pageSource == null)
    ) {
      "Reader Fetch initial state is not fresh"
    }
  }

  companion object {
    /** Creates the only valid fresh state for one Android request. */
    fun initial(force: Boolean, skipHath: Boolean, originalRequested: Boolean) =
      ReaderFetchState(
        phase = ReaderFetchPhase.INITIAL,
        attempt = 0,
        forceHtml = false,
        force = force,
        skipHath = skipHath,
        originalRequested = originalRequested,
        showKeyAvailable = false,
        pageSource = null,
      )
  }
}

/** Stable event discriminants accepted by the V1 reader fetch reducer. */
@Serializable
internal enum class ReaderFetchEventKind {
  @SerialName("start") START,
  @SerialName("cacheHit") CACHE_HIT,
  @SerialName("cacheMiss") CACHE_MISS,
  @SerialName("tokensReady") TOKENS_READY,
  @SerialName("tokensMissing") TOKENS_MISSING,
  @SerialName("pageReady") PAGE_READY,
  @SerialName("apiRecoverableFailure") API_RECOVERABLE_FAILURE,
  @SerialName("outerFailureRaised") OUTER_FAILURE_RAISED,
  @SerialName("outerFailureClassified") OUTER_FAILURE_CLASSIFIED,
  @SerialName("originalReady") ORIGINAL_READY,
  @SerialName("imageSucceeded") IMAGE_SUCCEEDED,
  @SerialName("imageFailed") IMAGE_FAILED,
}

/**
 * One sanitized Android observation applied to [ReaderFetchState].
 *
 * Nullable fields form an internally tagged union keyed by [kind]. No exception text, URL, token,
 * path, response body, cookie, gallery identity, or page identity enters this contract.
 */
@Serializable
internal data class ReaderFetchEvent(
  @SerialName("kind") val kind: ReaderFetchEventKind,
  @SerialName("source") val source: ReaderFetchPageSource? = null,
  @SerialName("originAvailable") val originAvailable: Boolean? = null,
  @SerialName("skipKeyAvailable") val skipKeyAvailable: Boolean? = null,
  @SerialName("showKeyAvailable") val showKeyAvailable: Boolean? = null,
  @SerialName("fatal") val fatal: Boolean? = null,
  @SerialName("invalidPage") val invalidPage: Boolean? = null,
  @SerialName("failure") val failure: ReaderFetchImageFailure? = null,
) {
  init {
    val noFields =
      source == null &&
        originAvailable == null &&
        skipKeyAvailable == null &&
        showKeyAvailable == null &&
        fatal == null &&
        invalidPage == null &&
        failure == null
    val shapeIsValid =
      when (kind) {
        ReaderFetchEventKind.START,
        ReaderFetchEventKind.CACHE_HIT,
        ReaderFetchEventKind.CACHE_MISS,
        ReaderFetchEventKind.TOKENS_MISSING,
        ReaderFetchEventKind.API_RECOVERABLE_FAILURE,
        ReaderFetchEventKind.ORIGINAL_READY,
        ReaderFetchEventKind.IMAGE_SUCCEEDED -> noFields
        ReaderFetchEventKind.TOKENS_READY ->
          source == null &&
            originAvailable == null &&
            skipKeyAvailable == null &&
            showKeyAvailable != null &&
            fatal == null &&
            invalidPage == null &&
            failure == null
        ReaderFetchEventKind.PAGE_READY ->
          source != null &&
            originAvailable != null &&
            skipKeyAvailable != null &&
            showKeyAvailable != null &&
            fatal == null &&
            invalidPage == null &&
            failure == null
        ReaderFetchEventKind.OUTER_FAILURE_RAISED ->
          source == null &&
            originAvailable == null &&
            skipKeyAvailable == null &&
            showKeyAvailable == null &&
            fatal != null &&
            invalidPage == null &&
            failure == null
        ReaderFetchEventKind.OUTER_FAILURE_CLASSIFIED ->
          source == null &&
            originAvailable == null &&
            skipKeyAvailable == null &&
            showKeyAvailable == null &&
            fatal == null &&
            invalidPage != null &&
            failure == null
        ReaderFetchEventKind.IMAGE_FAILED ->
          source == null &&
            originAvailable == null &&
            skipKeyAvailable == null &&
            showKeyAvailable == null &&
            fatal == null &&
            invalidPage == null &&
            failure != null
      }
    require(shapeIsValid) { "Reader Fetch event fields do not match the event kind" }
  }

  companion object {
    fun start() = ReaderFetchEvent(ReaderFetchEventKind.START)

    fun cacheHit() = ReaderFetchEvent(ReaderFetchEventKind.CACHE_HIT)

    fun cacheMiss() = ReaderFetchEvent(ReaderFetchEventKind.CACHE_MISS)

    fun tokensReady(showKeyAvailable: Boolean) =
      ReaderFetchEvent(
        kind = ReaderFetchEventKind.TOKENS_READY,
        showKeyAvailable = showKeyAvailable,
      )

    fun tokensMissing() = ReaderFetchEvent(ReaderFetchEventKind.TOKENS_MISSING)

    fun pageReady(
      source: ReaderFetchPageSource,
      originAvailable: Boolean,
      skipKeyAvailable: Boolean,
      showKeyAvailable: Boolean,
    ) =
      ReaderFetchEvent(
        kind = ReaderFetchEventKind.PAGE_READY,
        source = source,
        originAvailable = originAvailable,
        skipKeyAvailable = skipKeyAvailable,
        showKeyAvailable = showKeyAvailable,
      )

    fun apiRecoverableFailure() = ReaderFetchEvent(ReaderFetchEventKind.API_RECOVERABLE_FAILURE)

    fun outerFailureRaised(fatal: Boolean) =
      ReaderFetchEvent(kind = ReaderFetchEventKind.OUTER_FAILURE_RAISED, fatal = fatal)

    fun outerFailureClassified(invalidPage: Boolean) =
      ReaderFetchEvent(
        kind = ReaderFetchEventKind.OUTER_FAILURE_CLASSIFIED,
        invalidPage = invalidPage,
      )

    fun originalReady() = ReaderFetchEvent(ReaderFetchEventKind.ORIGINAL_READY)

    fun imageSucceeded() = ReaderFetchEvent(ReaderFetchEventKind.IMAGE_SUCCEEDED)

    fun imageFailed(failure: ReaderFetchImageFailure) =
      ReaderFetchEvent(kind = ReaderFetchEventKind.IMAGE_FAILED, failure = failure)
  }
}

/** Stable ordered action discriminants emitted by the V1 reader fetch reducer. */
@Serializable
internal enum class ReaderFetchActionKind {
  @SerialName("checkCache") CHECK_CACHE,
  @SerialName("acquireTokens") ACQUIRE_TOKENS,
  @SerialName("setDownloading") SET_DOWNLOADING,
  @SerialName("setFinished") SET_FINISHED,
  @SerialName("setFailed") SET_FAILED,
  @SerialName("fetchPage") FETCH_PAGE,
  @SerialName("resolveOriginal") RESOLVE_ORIGINAL,
  @SerialName("downloadImage") DOWNLOAD_IMAGE,
  @SerialName("removeIntermediateFiles") REMOVE_INTERMEDIATE_FILES,
  @SerialName("captureCurrentError") CAPTURE_CURRENT_ERROR,
  @SerialName("notifyFatal") NOTIFY_FATAL,
  @SerialName("invalidateToken") INVALIDATE_TOKEN,
  @SerialName("rethrowCancellation") RETHROW_CANCELLATION,
}

/** One strictly shaped, ordered Android effect emitted by the reducer. */
@Serializable
internal data class ReaderFetchAction(
  @SerialName("kind") val kind: ReaderFetchActionKind,
  @SerialName("error") val error: ReaderFetchErrorSource? = null,
  @SerialName("source") val source: ReaderFetchPageSource? = null,
  @SerialName("appendSkipKey") val appendSkipKey: Boolean? = null,
  @SerialName("target") val target: ReaderFetchImageTarget? = null,
) {
  init {
    val noFields = error == null && source == null && appendSkipKey == null && target == null
    val shapeIsValid =
      when (kind) {
        ReaderFetchActionKind.CHECK_CACHE,
        ReaderFetchActionKind.ACQUIRE_TOKENS,
        ReaderFetchActionKind.SET_DOWNLOADING,
        ReaderFetchActionKind.SET_FINISHED,
        ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES,
        ReaderFetchActionKind.CAPTURE_CURRENT_ERROR,
        ReaderFetchActionKind.NOTIFY_FATAL,
        ReaderFetchActionKind.INVALIDATE_TOKEN,
        ReaderFetchActionKind.RETHROW_CANCELLATION -> noFields
        ReaderFetchActionKind.SET_FAILED ->
          error != null && source == null && appendSkipKey == null && target == null
        ReaderFetchActionKind.FETCH_PAGE ->
          error == null && source != null && appendSkipKey == null && target == null
        ReaderFetchActionKind.RESOLVE_ORIGINAL ->
          error == null && source == null && appendSkipKey != null && target == null
        ReaderFetchActionKind.DOWNLOAD_IMAGE ->
          error == null && source == null && appendSkipKey == null && target != null
      }
    require(shapeIsValid) { "Reader Fetch action fields do not match the action kind" }
  }

  companion object {
    fun checkCache() = ReaderFetchAction(ReaderFetchActionKind.CHECK_CACHE)

    fun acquireTokens() = ReaderFetchAction(ReaderFetchActionKind.ACQUIRE_TOKENS)

    fun setDownloading() = ReaderFetchAction(ReaderFetchActionKind.SET_DOWNLOADING)

    fun setFinished() = ReaderFetchAction(ReaderFetchActionKind.SET_FINISHED)

    fun setFailed(error: ReaderFetchErrorSource) =
      ReaderFetchAction(kind = ReaderFetchActionKind.SET_FAILED, error = error)

    fun fetchPage(source: ReaderFetchPageSource) =
      ReaderFetchAction(kind = ReaderFetchActionKind.FETCH_PAGE, source = source)

    fun resolveOriginal(appendSkipKey: Boolean) =
      ReaderFetchAction(
        kind = ReaderFetchActionKind.RESOLVE_ORIGINAL,
        appendSkipKey = appendSkipKey,
      )

    fun downloadImage(target: ReaderFetchImageTarget) =
      ReaderFetchAction(kind = ReaderFetchActionKind.DOWNLOAD_IMAGE, target = target)

    fun removeIntermediateFiles() =
      ReaderFetchAction(ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES)

    fun captureCurrentError() = ReaderFetchAction(ReaderFetchActionKind.CAPTURE_CURRENT_ERROR)

    fun notifyFatal() = ReaderFetchAction(ReaderFetchActionKind.NOTIFY_FATAL)

    fun invalidateToken() = ReaderFetchAction(ReaderFetchActionKind.INVALIDATE_TOKEN)

    fun rethrowCancellation() = ReaderFetchAction(ReaderFetchActionKind.RETHROW_CANCELLATION)
  }
}

/** Deterministic next state and exact ordered Android effects. */
@Serializable
internal data class ReaderFetchPlan(
  @SerialName("state") val state: ReaderFetchState,
  @SerialName("actions") val actions: List<ReaderFetchAction>,
) {
  init {
    require(actions.isNotEmpty()) { "Reader Fetch plan must contain an action" }
  }
}

/** Validates and consumes cleanup that Android completed before a fallible planner transition. */
internal fun ReaderFetchPlan.consumePreappliedImageCleanup(): ReaderFetchPlan {
  check(actions.firstOrNull()?.kind == ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES) {
    "Reader Fetch pre-applied image cleanup requires a leading cleanup action"
  }
  check(actions.count { it.kind == ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES } == 1) {
    "Reader Fetch pre-applied image cleanup action must occur exactly once"
  }
  check(actions.size > 1) {
    "Reader Fetch pre-applied image cleanup requires a remaining action"
  }
  return copy(actions = actions.drop(1))
}

/** Versioned V1 request consumed by `eh_domain::reader::fetch`. */
@Serializable
internal data class ReaderFetchRequest(
  @SerialName("version") val version: Int,
  @SerialName("state") val state: ReaderFetchState,
  @SerialName("event") val event: ReaderFetchEvent,
) {
  init {
    require(version == READER_FETCH_API_VERSION) { "Unsupported Reader Fetch API version" }
  }
}

/** A session-locked source of complete reader fetch plans. */
internal fun interface ReaderFetchPlanner {
  fun next(state: ReaderFetchState, event: ReaderFetchEvent): ReaderFetchPlan
}

/** Selected session planner and its already-computed Start plan. */
internal data class ReaderFetchPlannerSelection(
  val planner: ReaderFetchPlanner,
  val startPlan: ReaderFetchPlan,
)

/** Independent Kotlin translation of the frozen V1 legacy reader-fetch transition table. */
internal fun legacyPlanReaderFetch(
  state: ReaderFetchState,
  event: ReaderFetchEvent,
): ReaderFetchPlan =
  when (state.phase) {
    ReaderFetchPhase.INITIAL ->
      when (event.kind) {
        ReaderFetchEventKind.START ->
          if (state.force) {
            plan(
              state.copy(phase = ReaderFetchPhase.ACQUIRING_TOKENS),
              ReaderFetchAction.acquireTokens(),
            )
          } else {
            plan(
              state.copy(phase = ReaderFetchPhase.CHECKING_CACHE),
              ReaderFetchAction.checkCache(),
            )
          }
        else -> invalidTransition()
      }
    ReaderFetchPhase.CHECKING_CACHE ->
      when (event.kind) {
        ReaderFetchEventKind.CACHE_HIT ->
          plan(state.copy(phase = ReaderFetchPhase.FINISHED), ReaderFetchAction.setFinished())
        ReaderFetchEventKind.CACHE_MISS ->
          plan(
            state.copy(phase = ReaderFetchPhase.ACQUIRING_TOKENS),
            ReaderFetchAction.acquireTokens(),
          )
        else -> invalidTransition()
      }
    ReaderFetchPhase.ACQUIRING_TOKENS ->
      when (event.kind) {
        ReaderFetchEventKind.TOKENS_MISSING ->
          plan(
            state.copy(phase = ReaderFetchPhase.FAILED),
            ReaderFetchAction.setFailed(ReaderFetchErrorSource.P_TOKEN_MESSAGE),
          )
        ReaderFetchEventKind.TOKENS_READY -> {
          val showKeyAvailable = checkNotNull(event.showKeyAvailable)
          val next = state.copy(showKeyAvailable = showKeyAvailable)
          val source = selectedPageSource(next)
          ReaderFetchPlan(
            state =
              next.copy(
                phase = ReaderFetchPhase.FETCHING_PAGE,
                pageSource = source,
              ),
            actions =
              listOf(
                ReaderFetchAction.setDownloading(),
                ReaderFetchAction.fetchPage(source),
              ),
          )
        }
        else -> invalidTransition()
      }
    ReaderFetchPhase.FETCHING_PAGE ->
      when (event.kind) {
        ReaderFetchEventKind.API_RECOVERABLE_FAILURE -> {
          require(state.pageSource == ReaderFetchPageSource.API) {
            "Reader Fetch API recovery requires the API source"
          }
          retryPageFetch(state.copy(forceHtml = true), emptyList())
        }
        ReaderFetchEventKind.PAGE_READY -> pageReadyPlan(state, event)
        ReaderFetchEventKind.OUTER_FAILURE_RAISED -> outerFailurePlan(state, event)
        else -> invalidTransition()
      }
    ReaderFetchPhase.RESOLVING_ORIGINAL ->
      when (event.kind) {
        ReaderFetchEventKind.ORIGINAL_READY ->
          plan(
            state.copy(phase = ReaderFetchPhase.DOWNLOADING_IMAGE),
            ReaderFetchAction.downloadImage(ReaderFetchImageTarget.ORIGINAL),
          )
        ReaderFetchEventKind.OUTER_FAILURE_RAISED -> outerFailurePlan(state, event)
        else -> invalidTransition()
      }
    ReaderFetchPhase.DOWNLOADING_IMAGE ->
      when (event.kind) {
        ReaderFetchEventKind.IMAGE_SUCCEEDED ->
          plan(state.copy(phase = ReaderFetchPhase.FINISHED), ReaderFetchAction.setFinished())
        ReaderFetchEventKind.IMAGE_FAILED -> imageFailurePlan(state, checkNotNull(event.failure))
        else -> invalidTransition()
      }
    ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE ->
      when (event.kind) {
        ReaderFetchEventKind.OUTER_FAILURE_CLASSIFIED -> {
          val actions = buildList {
            if (checkNotNull(event.invalidPage)) add(ReaderFetchAction.invalidateToken())
            add(ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED))
          }
          ReaderFetchPlan(state.copy(phase = ReaderFetchPhase.FAILED), actions)
        }
        else -> invalidTransition()
      }
    ReaderFetchPhase.FINISHED,
    ReaderFetchPhase.FAILED,
    ReaderFetchPhase.CANCELLED -> invalidTransition()
  }

/** Encodes one transition, invokes Rust, and validates the versioned response envelope. */
internal fun planReaderFetch(
  state: ReaderFetchState,
  event: ReaderFetchEvent,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanReaderFetchV1,
): ReaderFetchPlan {
  val request =
    rustCbor.encodeToByteArray(
      ReaderFetchRequest(
        version = READER_FETCH_API_VERSION,
        state = state,
        event = event,
      )
    )
  return invokeRustCbor<ApiEnvelope<ReaderFetchPlan>>(
      request,
      operation = "Reader Fetch",
      nativeCall = nativeCall,
    )
    .toReaderFetchPlan()
}

/** Selects one implementation before the first platform effect and computes Start with it. */
internal fun selectReaderFetchPlanner(
  enabled: Boolean,
  initialState: ReaderFetchState,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanReaderFetchV1,
): ReaderFetchPlannerSelection {
  val start = ReaderFetchEvent.start()
  val planner = if (enabled) RustReaderFetchPlanner(nativeCall) else LegacyReaderFetchPlanner
  return ReaderFetchPlannerSelection(planner, planner.next(initialState, start))
}

private object LegacyReaderFetchPlanner : ReaderFetchPlanner {
  override fun next(state: ReaderFetchState, event: ReaderFetchEvent) =
    legacyPlanReaderFetch(state, event)
}

private class RustReaderFetchPlanner(private val nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int) :
  ReaderFetchPlanner {
  override fun next(state: ReaderFetchState, event: ReaderFetchEvent) =
    planReaderFetch(state, event, nativeCall)
}

private fun ApiEnvelope<ReaderFetchPlan>.toReaderFetchPlan(): ReaderFetchPlan {
  require(version == READER_FETCH_API_VERSION) { "Unsupported Reader Fetch API version" }
  require(module == READER_FETCH_MODULE) { "Unexpected Rust Reader Fetch module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Rust Reader Fetch response has no payload" }
}

private fun pageReadyPlan(
  state: ReaderFetchState,
  event: ReaderFetchEvent,
): ReaderFetchPlan {
  val source = checkNotNull(event.source)
  require(state.pageSource == source) {
    "Reader Fetch page result source does not match the request"
  }
  val showKeyAvailable = checkNotNull(event.showKeyAvailable)
  if (state.attempt == 0 && state.skipHath) {
    return plan(
      state.copy(
        phase = ReaderFetchPhase.FETCHING_PAGE,
        attempt = 1,
        forceHtml = true,
        showKeyAvailable = showKeyAvailable,
        pageSource = ReaderFetchPageSource.HTML,
      ),
      ReaderFetchAction.fetchPage(ReaderFetchPageSource.HTML),
    )
  }
  return if (state.originalRequested && checkNotNull(event.originAvailable)) {
    plan(
      state.copy(
        phase = ReaderFetchPhase.RESOLVING_ORIGINAL,
        showKeyAvailable = showKeyAvailable,
        pageSource = null,
      ),
      ReaderFetchAction.resolveOriginal(
        appendSkipKey = state.attempt == 1 && checkNotNull(event.skipKeyAvailable)
      ),
    )
  } else {
    plan(
      state.copy(
        phase = ReaderFetchPhase.DOWNLOADING_IMAGE,
        forceHtml = true,
        showKeyAvailable = showKeyAvailable,
        pageSource = null,
      ),
      ReaderFetchAction.downloadImage(ReaderFetchImageTarget.RESIZED),
    )
  }
}

private fun outerFailurePlan(
  state: ReaderFetchState,
  event: ReaderFetchEvent,
): ReaderFetchPlan {
  val actions = buildList {
    if (checkNotNull(event.fatal)) add(ReaderFetchAction.notifyFatal())
    add(ReaderFetchAction.captureCurrentError())
  }
  return ReaderFetchPlan(
    state = state.copy(phase = ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE, pageSource = null),
    actions = actions,
  )
}

private fun imageFailurePlan(
  state: ReaderFetchState,
  failure: ReaderFetchImageFailure,
): ReaderFetchPlan =
  when (failure) {
    ReaderFetchImageFailure.CANCELLATION ->
      ReaderFetchPlan(
        state.copy(phase = ReaderFetchPhase.CANCELLED),
        listOf(
          ReaderFetchAction.removeIntermediateFiles(),
          ReaderFetchAction.rethrowCancellation(),
        ),
      )
    ReaderFetchImageFailure.FILE_NOT_FOUND ->
      ReaderFetchPlan(
        state.copy(phase = ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE),
        listOf(
          ReaderFetchAction.removeIntermediateFiles(),
          ReaderFetchAction.captureCurrentError(),
        ),
      )
    ReaderFetchImageFailure.RECOVERABLE ->
      retryPageFetch(
        state,
        listOf(
          ReaderFetchAction.removeIntermediateFiles(),
          ReaderFetchAction.captureCurrentError(),
        ),
      )
  }

private fun retryPageFetch(
  state: ReaderFetchState,
  precedingActions: List<ReaderFetchAction>,
): ReaderFetchPlan {
  return if (state.attempt < MAX_READER_FETCH_ATTEMPT) {
    val source = selectedPageSource(state)
    ReaderFetchPlan(
      state =
        state.copy(
          phase = ReaderFetchPhase.FETCHING_PAGE,
          attempt = state.attempt + 1,
          pageSource = source,
        ),
      actions = precedingActions + ReaderFetchAction.fetchPage(source),
    )
  } else {
    ReaderFetchPlan(
      state = state.copy(phase = ReaderFetchPhase.FAILED, pageSource = null),
      actions = precedingActions + ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
    )
  }
}

private fun selectedPageSource(state: ReaderFetchState) =
  if (state.forceHtml || !state.showKeyAvailable) {
    ReaderFetchPageSource.HTML
  } else {
    ReaderFetchPageSource.API
  }

private fun plan(state: ReaderFetchState, action: ReaderFetchAction) =
  ReaderFetchPlan(state, listOf(action))

private fun invalidTransition(): Nothing =
  throw IllegalArgumentException("Reader Fetch event is invalid for the current phase")

/** Invokes the V1 `eh_domain::reader::fetch` reducer using separate direct buffers. */
private external fun nativePlanReaderFetchV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val READER_FETCH_API_VERSION = 1
internal const val READER_FETCH_MODULE = "readerFetch"
internal const val MAX_READER_FETCH_ATTEMPT = 2
