package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class DailyCheckBridgeTest {
  @Test
  fun legacyPlannerPreservesUtcDayAndEligibilityRules() {
    assertEquals(expectedPlan, planLegacyDailyCheck(request))
    assertEquals(
      DailyCheckPlan(today = 11, shouldCheck = false),
      planLegacyDailyCheck(request.copy(signedIn = false)),
    )
    assertEquals(
      DailyCheckPlan(today = 11, shouldCheck = false),
      planLegacyDailyCheck(request.copy(lastDawnDays = 11)),
    )
  }

  @Test
  fun selectorRunsOnlyTheCapturedBackendAndPropagatesFailures() {
    var legacyCalls = 0
    var rustCalls = 0
    assertEquals(
      expectedPlan,
      selectDailyCheckPlan(
        backend = CoreBackend.LEGACY,
        legacyPlanner = {
          legacyCalls += 1
          expectedPlan
        },
        rustPlanner = {
          rustCalls += 1
          expectedPlan.copy(today = 12)
        },
      ),
    )
    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)

    assertFailsWith<LinkageError> {
      selectDailyCheckPlan(
        backend = CoreBackend.RUST,
        legacyPlanner = {
          legacyCalls += 1
          expectedPlan
        },
        rustPlanner = { throw LinkageError("missing") },
      )
    }
    assertEquals(1, legacyCalls)
  }

  @Test
  fun nativeCallbackReceivesOneCompleteRequestAndReturnsItsPlan() {
    var calls = 0
    val actual =
      planDailyCheck(request) { input: ByteBuffer, size: Int, output: ByteBuffer ->
        calls += 1
        val requestBytes = ByteArray(size).also(input::get)
        assertEquals(request, rustCbor.decodeFromByteArray<DailyCheckRequest>(requestBytes))
        val response =
          rustCbor.encodeToByteArray(
            ApiEnvelope(
              version = DAILY_CHECK_API_VERSION,
              module = DAILY_CHECK_MODULE,
              payload = expectedPlan,
            )
          )
        output.put(response)
        response.size
      }

    assertEquals(expectedPlan, actual)
    assertEquals(1, calls)
  }

  @Test
  fun bridgeRejectsBadMetadataMissingPayloadAndImpossibleEffects() {
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(ApiEnvelope(2, DAILY_CHECK_MODULE, expectedPlan))
    }
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(ApiEnvelope(1, "other", expectedPlan))
    }
    assertFails {
      callWithEnvelope(ApiEnvelope<DailyCheckPlan>(1, DAILY_CHECK_MODULE))
    }
    assertFailsWith<IllegalArgumentException> {
      callWithEnvelope(
        ApiEnvelope(1, DAILY_CHECK_MODULE, expectedPlan.copy(today = request.lastDawnDays))
      )
    }
  }

  private fun callWithEnvelope(envelope: ApiEnvelope<DailyCheckPlan>) =
    planDailyCheck(request) { _: ByteBuffer, _: Int, output: ByteBuffer ->
      val response = rustCbor.encodeToByteArray(envelope)
      output.put(response)
      response.size
    }

  private companion object {
    val request =
      DailyCheckRequest(
        version = DAILY_CHECK_API_VERSION,
        signedIn = true,
        lastDawnDays = 10,
        nowEpochSeconds = 11L * 86_400L,
      )
    val expectedPlan = DailyCheckPlan(today = 11, shouldCheck = true)
  }
}
