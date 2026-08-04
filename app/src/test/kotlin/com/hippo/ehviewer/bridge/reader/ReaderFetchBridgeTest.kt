package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertTrue
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class ReaderFetchBridgeTest {
  @Test
  fun requestAndResponseRustGoldensEncodeAndDecodeExactly() {
    val request =
      ReaderFetchRequest(
        version = 1,
        state = fetching(attempt = 1, originalRequested = true),
        event =
          ReaderFetchEvent.pageReady(
            source = ReaderFetchPageSource.API,
            originAvailable = true,
            skipKeyAvailable = true,
            showKeyAvailable = true,
          ),
      )
    val plan = legacyPlanReaderFetch(request.state, request.event)
    val response = successEnvelope(plan)
    val rustSuccessWire =
      RustSuccessEnvelope(
        version = READER_FETCH_API_VERSION,
        module = READER_FETCH_MODULE,
        payload = plan,
        error = null,
      )

    assertEquals(REQUEST_GOLDEN_CBOR_HEX, rustCbor.encodeToByteArray(request).toHex())
    assertEquals(RESPONSE_GOLDEN_CBOR_HEX, rustCbor.encodeToByteArray(rustSuccessWire).toHex())
    assertEquals(
      request,
      rustCbor.decodeFromByteArray<ReaderFetchRequest>(REQUEST_GOLDEN_CBOR_HEX.decodeHex()),
    )
    assertEquals(
      response,
      rustCbor.decodeFromByteArray<ApiEnvelope<ReaderFetchPlan>>(
        RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      ),
    )
  }

  @Test
  fun everyClosedEnumUsesTheExactRustWireName() {
    assertWireNames(
      ReaderFetchPhase.INITIAL to "initial",
      ReaderFetchPhase.CHECKING_CACHE to "checkingCache",
      ReaderFetchPhase.ACQUIRING_TOKENS to "acquiringTokens",
      ReaderFetchPhase.FETCHING_PAGE to "fetchingPage",
      ReaderFetchPhase.RESOLVING_ORIGINAL to "resolvingOriginal",
      ReaderFetchPhase.DOWNLOADING_IMAGE to "downloadingImage",
      ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE to "classifyingOuterFailure",
      ReaderFetchPhase.FINISHED to "finished",
      ReaderFetchPhase.FAILED to "failed",
      ReaderFetchPhase.CANCELLED to "cancelled",
    )
    assertWireNames(
      ReaderFetchPageSource.HTML to "html",
      ReaderFetchPageSource.API to "api",
    )
    assertWireNames(
      ReaderFetchImageTarget.ORIGINAL to "original",
      ReaderFetchImageTarget.RESIZED to "resized",
    )
    assertWireNames(
      ReaderFetchImageFailure.RECOVERABLE to "recoverable",
      ReaderFetchImageFailure.FILE_NOT_FOUND to "fileNotFound",
      ReaderFetchImageFailure.CANCELLATION to "cancellation",
    )
    assertWireNames(
      ReaderFetchErrorSource.P_TOKEN_MESSAGE to "pTokenMessage",
      ReaderFetchErrorSource.RETAINED to "retained",
    )
    assertWireNames(
      ReaderFetchEventKind.START to "start",
      ReaderFetchEventKind.CACHE_HIT to "cacheHit",
      ReaderFetchEventKind.CACHE_MISS to "cacheMiss",
      ReaderFetchEventKind.TOKENS_READY to "tokensReady",
      ReaderFetchEventKind.TOKENS_MISSING to "tokensMissing",
      ReaderFetchEventKind.PAGE_READY to "pageReady",
      ReaderFetchEventKind.API_RECOVERABLE_FAILURE to "apiRecoverableFailure",
      ReaderFetchEventKind.OUTER_FAILURE_RAISED to "outerFailureRaised",
      ReaderFetchEventKind.OUTER_FAILURE_CLASSIFIED to "outerFailureClassified",
      ReaderFetchEventKind.ORIGINAL_READY to "originalReady",
      ReaderFetchEventKind.IMAGE_SUCCEEDED to "imageSucceeded",
      ReaderFetchEventKind.IMAGE_FAILED to "imageFailed",
    )
    assertWireNames(
      ReaderFetchActionKind.CHECK_CACHE to "checkCache",
      ReaderFetchActionKind.ACQUIRE_TOKENS to "acquireTokens",
      ReaderFetchActionKind.SET_DOWNLOADING to "setDownloading",
      ReaderFetchActionKind.SET_FINISHED to "setFinished",
      ReaderFetchActionKind.SET_FAILED to "setFailed",
      ReaderFetchActionKind.FETCH_PAGE to "fetchPage",
      ReaderFetchActionKind.RESOLVE_ORIGINAL to "resolveOriginal",
      ReaderFetchActionKind.DOWNLOAD_IMAGE to "downloadImage",
      ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES to "removeIntermediateFiles",
      ReaderFetchActionKind.CAPTURE_CURRENT_ERROR to "captureCurrentError",
      ReaderFetchActionKind.NOTIFY_FATAL to "notifyFatal",
      ReaderFetchActionKind.INVALIDATE_TOKEN to "invalidateToken",
      ReaderFetchActionKind.RETHROW_CANCELLATION to "rethrowCancellation",
    )
  }

  @Test
  fun everyEventAndActionShapeRoundTripsAndIllegalFieldsAreRejected() {
    val events =
      listOf(
        ReaderFetchEvent.start(),
        ReaderFetchEvent.cacheHit(),
        ReaderFetchEvent.cacheMiss(),
        ReaderFetchEvent.tokensReady(false),
        ReaderFetchEvent.tokensMissing(),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.HTML, false, true, false),
        ReaderFetchEvent.apiRecoverableFailure(),
        ReaderFetchEvent.outerFailureRaised(false),
        ReaderFetchEvent.outerFailureClassified(true),
        ReaderFetchEvent.originalReady(),
        ReaderFetchEvent.imageSucceeded(),
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.FILE_NOT_FOUND),
      )
    events.forEach { event ->
      assertEquals(
        event,
        rustCbor.decodeFromByteArray<ReaderFetchEvent>(rustCbor.encodeToByteArray(event)),
      )
    }

    val actions =
      listOf(
        ReaderFetchAction.checkCache(),
        ReaderFetchAction.acquireTokens(),
        ReaderFetchAction.setDownloading(),
        ReaderFetchAction.setFinished(),
        ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
        ReaderFetchAction.fetchPage(ReaderFetchPageSource.API),
        ReaderFetchAction.resolveOriginal(false),
        ReaderFetchAction.downloadImage(ReaderFetchImageTarget.RESIZED),
        ReaderFetchAction.removeIntermediateFiles(),
        ReaderFetchAction.captureCurrentError(),
        ReaderFetchAction.notifyFatal(),
        ReaderFetchAction.invalidateToken(),
        ReaderFetchAction.rethrowCancellation(),
      )
    actions.forEach { action ->
      assertEquals(
        action,
        rustCbor.decodeFromByteArray<ReaderFetchAction>(rustCbor.encodeToByteArray(action)),
      )
    }

    assertFailsWith<IllegalArgumentException> {
      ReaderFetchEvent(ReaderFetchEventKind.START, fatal = false)
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchEvent(ReaderFetchEventKind.TOKENS_READY)
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchEvent(
        kind = ReaderFetchEventKind.PAGE_READY,
        source = ReaderFetchPageSource.API,
        originAvailable = true,
        skipKeyAvailable = false,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchEvent(
        kind = ReaderFetchEventKind.IMAGE_FAILED,
        failure = ReaderFetchImageFailure.RECOVERABLE,
        invalidPage = false,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchAction(ReaderFetchActionKind.SET_FAILED)
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchAction(ReaderFetchActionKind.CHECK_CACHE, source = ReaderFetchPageSource.HTML)
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchAction(
        ReaderFetchActionKind.RESOLVE_ORIGINAL,
        appendSkipKey = false,
        target = ReaderFetchImageTarget.ORIGINAL,
      )
    }
  }

  @Test
  fun stateAndEnvelopeConstructorsRejectEveryBoundedShapeViolation() {
    assertFailsWith<IllegalArgumentException> { initial().copy(attempt = -1) }
    assertFailsWith<IllegalArgumentException> { initial().copy(attempt = 3) }
    assertFailsWith<IllegalArgumentException> {
      initial().copy(phase = ReaderFetchPhase.FETCHING_PAGE)
    }
    assertFailsWith<IllegalArgumentException> {
      initial().copy(pageSource = ReaderFetchPageSource.HTML)
    }
    assertFailsWith<IllegalArgumentException> { ReaderFetchState.initial(false, true, false) }
    assertFailsWith<IllegalArgumentException> { initial().copy(forceHtml = true) }
    assertFailsWith<IllegalArgumentException> { initial().copy(showKeyAvailable = true) }
    assertFailsWith<IllegalArgumentException> { initial().copy(attempt = 1) }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchState.initial(force = true, skipHath = false, originalRequested = false)
        .copy(phase = ReaderFetchPhase.CHECKING_CACHE)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<ReaderFetchPlan>(
        version = 1,
        module = "readerFetch",
        payload = null,
        error = null,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
        version = 1,
        module = "readerFetch",
        payload = legacyPlanReaderFetch(initial(), ReaderFetchEvent.start()),
        error = RustError(RustErrorKind.ERROR, "reader fetch failure"),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchPlan(initial(), emptyList())
    }
  }

  @Test
  fun legacyStartCacheAndTokenTransitionsMatchTheFrozenTable() {
    val normalStart = legacyPlanReaderFetch(initial(), ReaderFetchEvent.start())
    assertEquals(ReaderFetchPhase.CHECKING_CACHE, normalStart.state.phase)
    assertEquals(listOf(ReaderFetchAction.checkCache()), normalStart.actions)
    assertEquals(
      listOf(ReaderFetchAction.setFinished()),
      legacyPlanReaderFetch(normalStart.state, ReaderFetchEvent.cacheHit()).actions,
    )
    val cacheMiss = legacyPlanReaderFetch(normalStart.state, ReaderFetchEvent.cacheMiss())
    assertEquals(ReaderFetchPhase.ACQUIRING_TOKENS, cacheMiss.state.phase)
    assertEquals(listOf(ReaderFetchAction.acquireTokens()), cacheMiss.actions)

    val forcedStart =
      legacyPlanReaderFetch(
        ReaderFetchState.initial(force = true, skipHath = false, originalRequested = true),
        ReaderFetchEvent.start(),
      )
    assertEquals(ReaderFetchPhase.ACQUIRING_TOKENS, forcedStart.state.phase)
    assertEquals(listOf(ReaderFetchAction.acquireTokens()), forcedStart.actions)
    assertEquals(
      listOf(ReaderFetchAction.setFailed(ReaderFetchErrorSource.P_TOKEN_MESSAGE)),
      legacyPlanReaderFetch(forcedStart.state, ReaderFetchEvent.tokensMissing()).actions,
    )

    val html = legacyPlanReaderFetch(cacheMiss.state, ReaderFetchEvent.tokensReady(false))
    assertEquals(ReaderFetchPageSource.HTML, html.state.pageSource)
    assertEquals(
      listOf(
        ReaderFetchAction.setDownloading(),
        ReaderFetchAction.fetchPage(ReaderFetchPageSource.HTML),
      ),
      html.actions,
    )
    val api = legacyPlanReaderFetch(cacheMiss.state, ReaderFetchEvent.tokensReady(true))
    assertEquals(ReaderFetchPageSource.API, api.state.pageSource)
  }

  @Test
  fun legacyPageSelectionPreservesSkipOriginalAndResizedSemantics() {
    val skipped =
      legacyPlanReaderFetch(
        fetching(
          attempt = 0,
          force = true,
          skipHath = true,
          originalRequested = true,
        ),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.API, true, true, true),
      )
    assertEquals(1, skipped.state.attempt)
    assertTrue(skipped.state.forceHtml)
    assertEquals(ReaderFetchPageSource.HTML, skipped.state.pageSource)
    assertEquals(
      listOf(ReaderFetchAction.fetchPage(ReaderFetchPageSource.HTML)),
      skipped.actions,
    )

    val original =
      legacyPlanReaderFetch(
        fetching(attempt = 1, originalRequested = true),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.API, true, true, true),
      )
    assertEquals(ReaderFetchPhase.RESOLVING_ORIGINAL, original.state.phase)
    assertEquals(listOf(ReaderFetchAction.resolveOriginal(true)), original.actions)
    val noAttemptOneSkipKey =
      legacyPlanReaderFetch(
        fetching(attempt = 0, originalRequested = true),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.API, true, true, true),
      )
    assertEquals(listOf(ReaderFetchAction.resolveOriginal(false)), noAttemptOneSkipKey.actions)

    val resized =
      legacyPlanReaderFetch(
        fetching(attempt = 0, originalRequested = false),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.API, true, true, true),
      )
    assertEquals(ReaderFetchPhase.DOWNLOADING_IMAGE, resized.state.phase)
    assertTrue(resized.state.forceHtml)
    assertEquals(
      listOf(ReaderFetchAction.downloadImage(ReaderFetchImageTarget.RESIZED)),
      resized.actions,
    )
  }

  @Test
  fun legacyRetriesAreBoundedAndNeverReplaceTheRetainedError() {
    val apiRetry =
      legacyPlanReaderFetch(fetching(attempt = 0), ReaderFetchEvent.apiRecoverableFailure())
    assertEquals(1, apiRetry.state.attempt)
    assertTrue(apiRetry.state.forceHtml)
    assertEquals(ReaderFetchPageSource.HTML, apiRetry.state.pageSource)
    assertEquals(
      listOf(ReaderFetchAction.fetchPage(ReaderFetchPageSource.HTML)),
      apiRetry.actions,
    )

    val exhausted =
      legacyPlanReaderFetch(fetching(attempt = 2), ReaderFetchEvent.apiRecoverableFailure())
    assertEquals(ReaderFetchPhase.FAILED, exhausted.state.phase)
    assertEquals(
      listOf(ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED)),
      exhausted.actions,
    )
    assertFalse(exhausted.actions.contains(ReaderFetchAction.captureCurrentError()))
  }

  @Test
  fun legacyOuterFailureClassificationPreservesExactActionOrder() {
    val fatal = legacyPlanReaderFetch(fetching(), ReaderFetchEvent.outerFailureRaised(fatal = true))
    assertEquals(ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE, fatal.state.phase)
    assertEquals(
      listOf(ReaderFetchAction.notifyFatal(), ReaderFetchAction.captureCurrentError()),
      fatal.actions,
    )
    assertEquals(
      listOf(
        ReaderFetchAction.invalidateToken(),
        ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
      ),
      legacyPlanReaderFetch(fatal.state, ReaderFetchEvent.outerFailureClassified(true)).actions,
    )

    val nonFatal =
      legacyPlanReaderFetch(
        resolvingOriginal(),
        ReaderFetchEvent.outerFailureRaised(fatal = false),
      )
    assertEquals(listOf(ReaderFetchAction.captureCurrentError()), nonFatal.actions)
    assertEquals(
      listOf(ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED)),
      legacyPlanReaderFetch(nonFatal.state, ReaderFetchEvent.outerFailureClassified(false)).actions,
    )
  }

  @Test
  fun legacyImageOutcomesPreserveCleanupRetryAndCancellationOrder() {
    val original = legacyPlanReaderFetch(resolvingOriginal(), ReaderFetchEvent.originalReady())
    assertEquals(
      listOf(ReaderFetchAction.downloadImage(ReaderFetchImageTarget.ORIGINAL)),
      original.actions,
    )
    assertEquals(
      listOf(ReaderFetchAction.setFinished()),
      legacyPlanReaderFetch(original.state, ReaderFetchEvent.imageSucceeded()).actions,
    )

    val recoverable =
      legacyPlanReaderFetch(
        downloading(attempt = 0),
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.RECOVERABLE),
      )
    assertEquals(1, recoverable.state.attempt)
    assertEquals(
      listOf(
        ReaderFetchAction.removeIntermediateFiles(),
        ReaderFetchAction.captureCurrentError(),
        ReaderFetchAction.fetchPage(ReaderFetchPageSource.API),
      ),
      recoverable.actions,
    )
    val exhausted =
      legacyPlanReaderFetch(
        downloading(attempt = 2),
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.RECOVERABLE),
      )
    assertEquals(
      listOf(
        ReaderFetchAction.removeIntermediateFiles(),
        ReaderFetchAction.captureCurrentError(),
        ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
      ),
      exhausted.actions,
    )

    val fileNotFound =
      legacyPlanReaderFetch(
        downloading(),
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.FILE_NOT_FOUND),
      )
    assertEquals(ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE, fileNotFound.state.phase)
    assertEquals(
      listOf(
        ReaderFetchAction.removeIntermediateFiles(),
        ReaderFetchAction.captureCurrentError(),
      ),
      fileNotFound.actions,
    )
    assertEquals(
      listOf(
        ReaderFetchAction.invalidateToken(),
        ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
      ),
      legacyPlanReaderFetch(
          fileNotFound.state,
          ReaderFetchEvent.outerFailureClassified(invalidPage = true),
        )
        .actions,
    )

    val cancelled =
      legacyPlanReaderFetch(
        downloading(),
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.CANCELLATION),
      )
    assertEquals(ReaderFetchPhase.CANCELLED, cancelled.state.phase)
    assertEquals(
      listOf(
        ReaderFetchAction.removeIntermediateFiles(),
        ReaderFetchAction.rethrowCancellation(),
      ),
      cancelled.actions,
    )
  }

  @Test
  fun preappliedImageCleanupConsumesOnlyTheLeadingActionAndPreservesRemainingOrder() {
    val plans =
      listOf(
        legacyPlanReaderFetch(
          downloading(attempt = 0),
          ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.RECOVERABLE),
        ),
        legacyPlanReaderFetch(
          downloading(attempt = 2),
          ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.RECOVERABLE),
        ),
        legacyPlanReaderFetch(
          downloading(),
          ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.CANCELLATION),
        ),
      )

    plans.forEach { plan ->
      val consumed = plan.consumePreappliedImageCleanup()

      assertEquals(plan.state, consumed.state)
      assertEquals(plan.actions.drop(1), consumed.actions)
    }
    assertEquals(
      listOf(
        ReaderFetchAction.captureCurrentError(),
        ReaderFetchAction.fetchPage(ReaderFetchPageSource.API),
      ),
      plans[0].consumePreappliedImageCleanup().actions,
    )
    assertEquals(
      listOf(
        ReaderFetchAction.captureCurrentError(),
        ReaderFetchAction.setFailed(ReaderFetchErrorSource.RETAINED),
      ),
      plans[1].consumePreappliedImageCleanup().actions,
    )
    assertEquals(
      listOf(ReaderFetchAction.rethrowCancellation()),
      plans[2].consumePreappliedImageCleanup().actions,
    )
  }

  @Test
  fun preappliedImageCleanupRejectsMissingOrWrongLeadingCleanup() {
    val missing =
      ReaderFetchPlan(
        downloading(),
        listOf(ReaderFetchAction.captureCurrentError()),
      )
    val wrongLeading =
      ReaderFetchPlan(
        downloading(),
        listOf(
          ReaderFetchAction.captureCurrentError(),
          ReaderFetchAction.removeIntermediateFiles(),
        ),
      )

    listOf(missing, wrongLeading).forEach { plan ->
      val failure = assertFailsWith<IllegalStateException> { plan.consumePreappliedImageCleanup() }
      assertEquals(
        "Reader Fetch pre-applied image cleanup requires a leading cleanup action",
        failure.message,
      )
    }
  }

  @Test
  fun preappliedImageCleanupRejectsDuplicateOrActionlessRemainders() {
    val duplicate =
      ReaderFetchPlan(
        downloading(),
        listOf(
          ReaderFetchAction.removeIntermediateFiles(),
          ReaderFetchAction.captureCurrentError(),
          ReaderFetchAction.removeIntermediateFiles(),
        ),
      )
    val duplicateFailure =
      assertFailsWith<IllegalStateException> { duplicate.consumePreappliedImageCleanup() }
    assertEquals(
      "Reader Fetch pre-applied image cleanup action must occur exactly once",
      duplicateFailure.message,
    )

    val actionless =
      ReaderFetchPlan(downloading(), listOf(ReaderFetchAction.removeIntermediateFiles()))
    val actionlessFailure =
      assertFailsWith<IllegalStateException> { actionless.consumePreappliedImageCleanup() }
    assertEquals(
      "Reader Fetch pre-applied image cleanup requires a remaining action",
      actionlessFailure.message,
    )
  }

  @Test
  fun legacyRejectsIllegalSourcesTransitionsAndTerminalEvents() {
    assertFailsWith<IllegalArgumentException> {
      legacyPlanReaderFetch(initial(), ReaderFetchEvent.cacheHit())
    }
    assertFailsWith<IllegalArgumentException> {
      legacyPlanReaderFetch(
        fetching(source = ReaderFetchPageSource.HTML, showKeyAvailable = false),
        ReaderFetchEvent.apiRecoverableFailure(),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      legacyPlanReaderFetch(
        fetching(),
        ReaderFetchEvent.pageReady(ReaderFetchPageSource.HTML, true, true, true),
      )
    }
    ReaderFetchPhase.entries
      .filter {
        it == ReaderFetchPhase.FINISHED ||
          it == ReaderFetchPhase.FAILED ||
          it == ReaderFetchPhase.CANCELLED
      }
      .forEach { terminal ->
        assertFailsWith<IllegalArgumentException> {
          legacyPlanReaderFetch(
            initial().copy(phase = terminal),
            ReaderFetchEvent.imageSucceeded(),
          )
        }
      }
  }

  @Test
  fun nativePlanningRejectsEnvelopeVersionModuleErrorAndMalformedBytes() {
    val state = initial()
    val event = ReaderFetchEvent.start()
    val expected = legacyPlanReaderFetch(state, event)

    assertFailsWith<IllegalArgumentException> {
      planReaderFetch(
        state,
        event,
        ScriptedNative({ envelopeBytes(expected, version = 2) })::invoke,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      planReaderFetch(
        state,
        event,
        ScriptedNative({ envelopeBytes(expected, module = "readerSchedule") })::invoke,
      )
    }
    val rustFailure = RustError(RustErrorKind.ERROR, "sanitized reader fetch failure")
    val failure =
      assertFailsWith<RustFailureException> {
        planReaderFetch(
          state,
          event,
          ScriptedNative({ errorEnvelopeBytes(rustFailure) })::invoke,
        )
      }
    assertEquals(rustFailure, failure.failure)
    assertFailsWith<Exception> {
      planReaderFetch(state, event, ScriptedNative({ byteArrayOf(0xa0.toByte()) })::invoke)
    }
    val rustPlan = mismatched(expected)
    assertEquals(
      rustPlan,
      planReaderFetch(state, event, ScriptedNative({ envelopeBytes(rustPlan) })::invoke),
    )
  }

  @Test
  fun disabledSelectionUsesLegacyWithoutAnyNativeCallOrSecondStart() {
    val native = ScriptedNative()
    val state = initial()
    val expectedStart = legacyPlanReaderFetch(state, ReaderFetchEvent.start())

    val selection = selectReaderFetchPlanner(false, state, native::invoke)

    assertEquals(expectedStart, selection.startPlan)
    assertEquals(0, native.calls)
    val next = selection.planner.next(selection.startPlan.state, ReaderFetchEvent.cacheMiss())
    assertEquals(
      legacyPlanReaderFetch(selection.startPlan.state, ReaderFetchEvent.cacheMiss()),
      next,
    )
    assertEquals(0, native.calls)
  }

  @Test
  fun successfulInitialRustPlanLocksTheSessionToStrictRustPlanning() {
    val state = initial()
    val expectedStart = legacyPlanReaderFetch(state, ReaderFetchEvent.start())
    val expectedNext = legacyPlanReaderFetch(expectedStart.state, ReaderFetchEvent.cacheMiss())
    val native =
      ScriptedNative(
        { request ->
          assertEquals(state, request.state)
          assertEquals(ReaderFetchEvent.start(), request.event)
          envelopeBytes(expectedStart)
        },
        { request ->
          assertEquals(expectedStart.state, request.state)
          assertEquals(ReaderFetchEvent.cacheMiss(), request.event)
          envelopeBytes(expectedNext)
        },
      )

    val selection = selectReaderFetchPlanner(true, state, native::invoke)

    assertEquals(expectedStart, selection.startPlan)
    assertEquals(1, native.calls)
    assertEquals(
      expectedNext,
      selection.planner.next(selection.startPlan.state, ReaderFetchEvent.cacheMiss()),
    )
    assertEquals(2, native.calls)
  }

  @Test
  fun initialRustFailuresPropagateWithoutLegacySelection() {
    assertInitialFailure<IllegalStateException> {
      throw IllegalStateException("contained test failure")
    }
    assertInitialFailure<UnsatisfiedLinkError> {
      throw UnsatisfiedLinkError("contained test linkage")
    }
    assertInitialFailure<Exception> { byteArrayOf(0xa0.toByte()) }
  }

  @Test
  fun fatalErrorAtTheInitialGatePropagates() {
    val native = ScriptedNative({ throw AssertionError("fatal test error") })

    assertFailsWith<AssertionError> {
      selectReaderFetchPlanner(true, initial(), native::invoke)
    }
    assertEquals(1, native.calls)
  }

  @Test
  fun selectedRustPlannerNeverFallsBackReplaysOrSwitchesAfterFailure() {
    assertStrictLaterFailure<IllegalStateException> {
      throw IllegalStateException("strict test failure")
    }
    assertStrictLaterFailure<UnsatisfiedLinkError> {
      throw UnsatisfiedLinkError("strict test linkage")
    }
    assertStrictLaterFailure<Exception> { byteArrayOf(0xa0.toByte()) }
  }

  @Test
  fun strictImageFailurePlanningPropagatesAfterCallerPreappliesCleanup() {
    val state = initial()
    val start = legacyPlanReaderFetch(state, ReaderFetchEvent.start())
    val downloading = downloading()
    val imageFailure = ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.RECOVERABLE)
    val native =
      ScriptedNative(
        { envelopeBytes(start) },
        { request ->
          assertEquals(downloading, request.state)
          assertEquals(imageFailure, request.event)
          throw UnsatisfiedLinkError("strict image failure linkage")
        },
      )
    val selection = selectReaderFetchPlanner(true, state, native::invoke)
    var cleanupCalls = 0

    cleanupCalls++
    assertFailsWith<UnsatisfiedLinkError> {
      selection.planner.next(downloading, imageFailure).consumePreappliedImageCleanup()
    }

    assertEquals(1, cleanupCalls)
    assertEquals(2, native.calls)
  }

  @Test
  fun wireSchemaAndDiagnosticsContainOnlySanitizedControlData() {
    val secret = "secret-token-cookie-url-path-body"
    val request =
      ReaderFetchRequest(
        version = 1,
        state = initial(),
        event = ReaderFetchEvent.start(),
      )
    val wireText = rustCbor.encodeToByteArray(request).toString(Charsets.ISO_8859_1).lowercase()
    listOf("cookie", "url", "path", "body", "exception", "galleryid", "pageindex").forEach {
      forbidden ->
      assertFalse(wireText.contains(forbidden))
    }

    val error = RustError(RustErrorKind.ERROR, secret)
    val thrown =
      assertFailsWith<RustFailureException> {
        planReaderFetch(
          initial(),
          ReaderFetchEvent.start(),
          ScriptedNative({ errorEnvelopeBytes(error) })::invoke,
        )
      }
    assertFalse(thrown.message.orEmpty().contains(secret))
    val transitionError =
      assertFailsWith<IllegalArgumentException> {
        legacyPlanReaderFetch(initial(), ReaderFetchEvent.cacheMiss())
      }
    assertEquals("Reader Fetch event is invalid for the current phase", transitionError.message)
  }

  private inline fun <reified T : Throwable> assertInitialFailure(
    noinline step: (ReaderFetchRequest) -> ByteArray
  ) {
    val native = ScriptedNative(step)

    assertFailsWith<T> { selectReaderFetchPlanner(true, initial(), native::invoke) }
    assertEquals(1, native.calls)
  }

  private inline fun <reified T : Throwable> assertStrictLaterFailure(
    noinline secondStep: (ReaderFetchRequest) -> ByteArray
  ) {
    val state = initial()
    val start = legacyPlanReaderFetch(state, ReaderFetchEvent.start())
    val native = ScriptedNative({ envelopeBytes(start) }, secondStep)
    val selection = selectReaderFetchPlanner(true, state, native::invoke)

    assertEquals(1, native.calls)
    assertFailsWith<T> {
      selection.planner.next(selection.startPlan.state, ReaderFetchEvent.cacheMiss())
    }
    assertEquals(2, native.calls)
  }

  private class ScriptedNative(vararg steps: (ReaderFetchRequest) -> ByteArray) {
    private val scriptedSteps = steps.toList()
    var calls = 0
      private set

    fun invoke(
      requestBuffer: ByteBuffer,
      requestSize: Int,
      responseBuffer: ByteBuffer,
    ): Int {
      val requestBytes = ByteArray(requestSize)
      requestBuffer.get(requestBytes)
      val request = rustCbor.decodeFromByteArray<ReaderFetchRequest>(requestBytes)
      val step = scriptedSteps.getOrElse(calls) { error("Unexpected Reader Fetch native replay") }
      calls++
      val response = step(request)
      responseBuffer.put(response)
      return response.size
    }
  }

  private companion object {
    const val REQUEST_GOLDEN_CBOR_HEX =
      "a36776657273696f6e01657374617465a86570686173656c6665746368696e67506167656761" +
        "7474656d70740169666f72636548746d6cf465666f726365f468736b697048617468f4716f72" +
        "6967696e616c526571756573746564f57073686f774b6579417661696c61626c65f56a706167" +
        "65536f7572636563617069656576656e74a5646b696e646970616765526561647966736f7572" +
        "6365636170696f6f726967696e417661696c61626c65f570736b69704b6579417661696c6162" +
        "6c65f57073686f774b6579417661696c61626c65f5"

    const val RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656b7265616465724665746368677061796c6f6164" +
        "a2657374617465a8657068617365717265736f6c76696e674f726967696e616c67617474656d" +
        "70740169666f72636548746d6cf465666f726365f468736b697048617468f4716f726967696e" +
        "616c526571756573746564f57073686f774b6579417661696c61626c65f56a70616765536f75" +
        "726365f667616374696f6e7381a2646b696e646f7265736f6c76654f726967696e616c6d6170" +
        "70656e64536b69704b6579f5656572726f72f6"
  }
}

private fun initial() =
  ReaderFetchState.initial(force = false, skipHath = false, originalRequested = true)

private fun fetching(
  attempt: Int = 0,
  forceHtml: Boolean = false,
  force: Boolean = false,
  skipHath: Boolean = false,
  originalRequested: Boolean = true,
  showKeyAvailable: Boolean = true,
  source: ReaderFetchPageSource =
    if (forceHtml || !showKeyAvailable) ReaderFetchPageSource.HTML else ReaderFetchPageSource.API,
) =
  ReaderFetchState(
    phase = ReaderFetchPhase.FETCHING_PAGE,
    attempt = attempt,
    forceHtml = forceHtml,
    force = force,
    skipHath = skipHath,
    originalRequested = originalRequested,
    showKeyAvailable = showKeyAvailable,
    pageSource = source,
  )

private fun resolvingOriginal() =
  ReaderFetchState(
    phase = ReaderFetchPhase.RESOLVING_ORIGINAL,
    attempt = 1,
    forceHtml = false,
    force = false,
    skipHath = false,
    originalRequested = true,
    showKeyAvailable = true,
    pageSource = null,
  )

private fun downloading(attempt: Int = 0) =
  ReaderFetchState(
    phase = ReaderFetchPhase.DOWNLOADING_IMAGE,
    attempt = attempt,
    forceHtml = false,
    force = false,
    skipHath = false,
    originalRequested = true,
    showKeyAvailable = true,
    pageSource = null,
  )

private fun successEnvelope(plan: ReaderFetchPlan) =
  ApiEnvelope(
    version = READER_FETCH_API_VERSION,
    module = READER_FETCH_MODULE,
    payload = plan,
    error = null,
  )

private fun envelopeBytes(
  plan: ReaderFetchPlan,
  version: Int = READER_FETCH_API_VERSION,
  module: String = READER_FETCH_MODULE,
) =
  rustCbor.encodeToByteArray(
    ApiEnvelope(version = version, module = module, payload = plan, error = null)
  )

private fun errorEnvelopeBytes(error: RustError) =
  rustCbor.encodeToByteArray(
    ApiEnvelope<ReaderFetchPlan>(
      version = READER_FETCH_API_VERSION,
      module = READER_FETCH_MODULE,
      payload = null,
      error = error,
    )
  )

private fun mismatched(expected: ReaderFetchPlan): ReaderFetchPlan {
  val replacement =
    if (expected.actions.first().kind == ReaderFetchActionKind.ACQUIRE_TOKENS) {
      ReaderFetchAction.checkCache()
    } else {
      ReaderFetchAction.acquireTokens()
    }
  val mismatched = expected.copy(actions = listOf(replacement))
  assertNotEquals(expected, mismatched)
  return mismatched
}

private inline fun <reified T> assertWireNames(vararg values: Pair<T, String>) {
  values.forEach { (value, wireName) ->
    val encoded = rustCbor.encodeToByteArray(value)
    assertEquals(wireName, encoded.copyOfRange(1, encoded.size).decodeToString())
    assertEquals(value, rustCbor.decodeFromByteArray<T>(encoded))
  }
}

private fun ByteArray.toHex() =
  joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

// A primitive nullable sentinel makes Kotlin emit the canonical CBOR null used by serde_cbor.
@Serializable
private data class RustSuccessEnvelope(
  val version: Int,
  val module: String,
  val payload: ReaderFetchPlan,
  val error: String?,
)
