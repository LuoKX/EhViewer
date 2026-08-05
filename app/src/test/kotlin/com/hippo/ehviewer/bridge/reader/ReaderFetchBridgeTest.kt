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
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class ReaderFetchBridgeTest {
  @Test
  fun requestAndResponseRustGoldensEncodeAndDecodeExactly() {
    val request =
      ReaderFetchRequest(
        version = READER_FETCH_API_VERSION,
        state = fetching(attempt = 1, originalRequested = true),
        event =
          ReaderFetchEvent.pageReady(
            source = ReaderFetchPageSource.API,
            originAvailable = true,
            skipKeyAvailable = true,
            showKeyAvailable = true,
          ),
      )
    val plan = goldenPlan()
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
  fun everyEventAndActionShapeRoundTripsAndRejectsIllegalFields() {
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
      ReaderFetchAction(ReaderFetchActionKind.SET_FAILED)
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderFetchAction(ReaderFetchActionKind.CHECK_CACHE, source = ReaderFetchPageSource.HTML)
    }
  }

  @Test
  fun stateAndEnvelopeConstructorsRejectBoundedShapeViolations() {
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
      ApiEnvelope(
        version = READER_FETCH_API_VERSION,
        module = READER_FETCH_MODULE,
        payload = startPlan(),
        error = RustError(RustErrorKind.ERROR, "reader fetch failure"),
      )
    }
    assertFailsWith<IllegalArgumentException> { ReaderFetchPlan(initial(), emptyList()) }
  }

  @Test
  fun preappliedImageCleanupConsumesOnlyOneLeadingAction() {
    val plan =
      ReaderFetchPlan(
        downloading(attempt = 1),
        listOf(
          ReaderFetchAction.removeIntermediateFiles(),
          ReaderFetchAction.captureCurrentError(),
          ReaderFetchAction.fetchPage(ReaderFetchPageSource.API),
        ),
      )

    assertEquals(
      plan.copy(actions = plan.actions.drop(1)),
      plan.consumePreappliedImageCleanup(),
    )
    assertFailsWith<IllegalStateException> {
      ReaderFetchPlan(downloading(), listOf(ReaderFetchAction.captureCurrentError()))
        .consumePreappliedImageCleanup()
    }
    assertFailsWith<IllegalStateException> {
      ReaderFetchPlan(
          downloading(),
          listOf(
            ReaderFetchAction.removeIntermediateFiles(),
            ReaderFetchAction.captureCurrentError(),
            ReaderFetchAction.removeIntermediateFiles(),
          ),
        )
        .consumePreappliedImageCleanup()
    }
    assertFailsWith<IllegalStateException> {
      ReaderFetchPlan(downloading(), listOf(ReaderFetchAction.removeIntermediateFiles()))
        .consumePreappliedImageCleanup()
    }
  }

  @Test
  fun nativePlanningValidatesEnvelopeAndAdoptsRustPayload() {
    val state = initial()
    val event = ReaderFetchEvent.start()
    val expected = startPlan()

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

    val adopted = expected.copy(actions = listOf(ReaderFetchAction.acquireTokens()))
    assertEquals(
      adopted,
      planReaderFetch(state, event, ScriptedNative({ envelopeBytes(adopted) })::invoke),
    )
  }

  @Test
  fun startedPlannerUsesRustExactlyOncePerTransition() {
    val state = initial()
    val expectedStart = startPlan()
    val expectedNext = cacheMissPlan()
    val native =
      ScriptedNative(
        { request ->
          assertEquals(
            ReaderFetchRequest(READER_FETCH_API_VERSION, state, ReaderFetchEvent.start()),
            request,
          )
          envelopeBytes(expectedStart)
        },
        { request ->
          assertEquals(
            ReaderFetchRequest(
              READER_FETCH_API_VERSION,
              expectedStart.state,
              ReaderFetchEvent.cacheMiss(),
            ),
            request,
          )
          envelopeBytes(expectedNext)
        },
      )

    val selection = startReaderFetchPlanner(state, native::invoke)

    assertEquals(expectedStart, selection.startPlan)
    assertEquals(1, native.calls)
    assertEquals(
      expectedNext,
      selection.planner.next(selection.startPlan.state, ReaderFetchEvent.cacheMiss()),
    )
    assertEquals(2, native.calls)
  }

  @Test
  fun rustFailuresPropagateWithoutReplay() {
    assertInitialFailure<IllegalStateException> {
      throw IllegalStateException("contained test failure")
    }
    assertInitialFailure<UnsatisfiedLinkError> {
      throw UnsatisfiedLinkError("contained test linkage")
    }
    assertInitialFailure<Exception> { byteArrayOf(0xa0.toByte()) }

    assertLaterFailure<IllegalStateException> {
      throw IllegalStateException("strict test failure")
    }
    assertLaterFailure<UnsatisfiedLinkError> {
      throw UnsatisfiedLinkError("strict test linkage")
    }
    assertLaterFailure<Exception> { byteArrayOf(0xa0.toByte()) }
  }

  @Test
  fun wireSchemaAndDiagnosticsContainOnlySanitizedControlData() {
    val secret = "secret-token-cookie-url-path-body"
    val request = ReaderFetchRequest(READER_FETCH_API_VERSION, initial(), ReaderFetchEvent.start())
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
  }

  private inline fun <reified T : Throwable> assertInitialFailure(
    noinline step: (ReaderFetchRequest) -> ByteArray
  ) {
    val native = ScriptedNative(step)
    assertFailsWith<T> { startReaderFetchPlanner(initial(), native::invoke) }
    assertEquals(1, native.calls)
  }

  private inline fun <reified T : Throwable> assertLaterFailure(
    noinline secondStep: (ReaderFetchRequest) -> ByteArray
  ) {
    val native = ScriptedNative({ envelopeBytes(startPlan()) }, secondStep)
    val selection = startReaderFetchPlanner(initial(), native::invoke)

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
  originalRequested: Boolean = true,
) =
  ReaderFetchState(
    phase = ReaderFetchPhase.FETCHING_PAGE,
    attempt = attempt,
    forceHtml = false,
    force = false,
    skipHath = false,
    originalRequested = originalRequested,
    showKeyAvailable = true,
    pageSource = ReaderFetchPageSource.API,
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

private fun startPlan() =
  ReaderFetchPlan(
    state = initial().copy(phase = ReaderFetchPhase.CHECKING_CACHE),
    actions = listOf(ReaderFetchAction.checkCache()),
  )

private fun cacheMissPlan() =
  ReaderFetchPlan(
    state =
      ReaderFetchState(
        phase = ReaderFetchPhase.ACQUIRING_TOKENS,
        attempt = 0,
        forceHtml = false,
        force = false,
        skipHath = false,
        originalRequested = true,
        showKeyAvailable = false,
        pageSource = null,
      ),
    actions = listOf(ReaderFetchAction.acquireTokens()),
  )

private fun goldenPlan() =
  ReaderFetchPlan(
    state =
      ReaderFetchState(
        phase = ReaderFetchPhase.RESOLVING_ORIGINAL,
        attempt = 1,
        forceHtml = false,
        force = false,
        skipHath = false,
        originalRequested = true,
        showKeyAvailable = true,
        pageSource = null,
      ),
    actions = listOf(ReaderFetchAction.resolveOriginal(true)),
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

private inline fun <reified T> assertWireNames(vararg values: Pair<T, String>) {
  values.forEach { (value, wireName) ->
    val encoded = rustCbor.encodeToByteArray(value)
    assertEquals(wireName, encoded.copyOfRange(1, encoded.size).decodeToString())
    assertEquals(value, rustCbor.decodeFromByteArray<T>(encoded))
  }
}

private fun ByteArray.toHex() =
  joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

@Serializable
private data class RustSuccessEnvelope(
  val version: Int,
  val module: String,
  val payload: ReaderFetchPlan,
  val error: String?,
)
