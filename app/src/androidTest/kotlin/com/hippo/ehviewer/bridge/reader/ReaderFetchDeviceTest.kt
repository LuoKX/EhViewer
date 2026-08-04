package com.hippo.ehviewer.bridge.reader

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI contract for the V1 reader fetch transition boundary. */
@RunWith(AndroidJUnit4::class)
class ReaderFetchDeviceTest {
  @Test
  fun plansACompleteOriginalImageSuccessSequenceThroughRealJni() {
    var plan =
      planReaderFetch(
        ReaderFetchState.initial(
          force = false,
          skipHath = false,
          originalRequested = true,
        ),
        ReaderFetchEvent.start(),
      )
    assertEquals(ReaderFetchPhase.CHECKING_CACHE, plan.state.phase)
    assertEquals(listOf(ReaderFetchActionKind.CHECK_CACHE), plan.actionKinds())

    plan = planReaderFetch(plan.state, ReaderFetchEvent.cacheMiss())
    assertEquals(ReaderFetchPhase.ACQUIRING_TOKENS, plan.state.phase)
    assertEquals(listOf(ReaderFetchActionKind.ACQUIRE_TOKENS), plan.actionKinds())

    plan = planReaderFetch(plan.state, ReaderFetchEvent.tokensReady(showKeyAvailable = true))
    assertEquals(ReaderFetchPhase.FETCHING_PAGE, plan.state.phase)
    assertEquals(ReaderFetchPageSource.API, plan.state.pageSource)
    assertEquals(
      listOf(ReaderFetchActionKind.SET_DOWNLOADING, ReaderFetchActionKind.FETCH_PAGE),
      plan.actionKinds(),
    )

    plan =
      planReaderFetch(
        plan.state,
        ReaderFetchEvent.pageReady(
          source = ReaderFetchPageSource.API,
          originAvailable = true,
          skipKeyAvailable = true,
          showKeyAvailable = true,
        ),
      )
    assertEquals(ReaderFetchPhase.RESOLVING_ORIGINAL, plan.state.phase)
    assertEquals(listOf(ReaderFetchActionKind.RESOLVE_ORIGINAL), plan.actionKinds())
    assertFalse(checkNotNull(plan.actions.single().appendSkipKey))

    plan = planReaderFetch(plan.state, ReaderFetchEvent.originalReady())
    assertEquals(ReaderFetchPhase.DOWNLOADING_IMAGE, plan.state.phase)
    assertEquals(ReaderFetchImageTarget.ORIGINAL, plan.actions.single().target)

    plan = planReaderFetch(plan.state, ReaderFetchEvent.imageSucceeded())
    assertEquals(ReaderFetchPhase.FINISHED, plan.state.phase)
    assertEquals(listOf(ReaderFetchActionKind.SET_FINISHED), plan.actionKinds())
  }

  @Test
  fun plansCleanupAndOuterClassificationFailureThroughRealJni() {
    var plan =
      planReaderFetch(
        ReaderFetchState.initial(
          force = true,
          skipHath = false,
          originalRequested = false,
        ),
        ReaderFetchEvent.start(),
      )
    assertEquals(listOf(ReaderFetchActionKind.ACQUIRE_TOKENS), plan.actionKinds())

    plan = planReaderFetch(plan.state, ReaderFetchEvent.tokensReady(showKeyAvailable = false))
    assertEquals(ReaderFetchPageSource.HTML, plan.state.pageSource)

    plan =
      planReaderFetch(
        plan.state,
        ReaderFetchEvent.pageReady(
          source = ReaderFetchPageSource.HTML,
          originAvailable = false,
          skipKeyAvailable = false,
          showKeyAvailable = false,
        ),
      )
    assertEquals(ReaderFetchImageTarget.RESIZED, plan.actions.single().target)

    plan =
      planReaderFetch(
        plan.state,
        ReaderFetchEvent.imageFailed(ReaderFetchImageFailure.FILE_NOT_FOUND),
      )
    assertEquals(ReaderFetchPhase.CLASSIFYING_OUTER_FAILURE, plan.state.phase)
    assertEquals(
      listOf(
        ReaderFetchActionKind.REMOVE_INTERMEDIATE_FILES,
        ReaderFetchActionKind.CAPTURE_CURRENT_ERROR,
      ),
      plan.actionKinds(),
    )

    plan =
      planReaderFetch(
        plan.state,
        ReaderFetchEvent.outerFailureClassified(invalidPage = true),
      )
    assertEquals(ReaderFetchPhase.FAILED, plan.state.phase)
    assertEquals(
      listOf(ReaderFetchActionKind.INVALIDATE_TOKEN, ReaderFetchActionKind.SET_FAILED),
      plan.actionKinds(),
    )
    assertEquals(ReaderFetchErrorSource.RETAINED, plan.actions.last().error)
  }

  private fun ReaderFetchPlan.actionKinds() = actions.map(ReaderFetchAction::kind)
}
