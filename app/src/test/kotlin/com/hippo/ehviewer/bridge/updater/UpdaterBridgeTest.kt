package com.hippo.ehviewer.bridge.updater

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.rustCbor
import java.io.IOException
import java.nio.ByteBuffer
import kotlin.test.*
import kotlin.time.Duration.Companion.days
import kotlin.time.Instant
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class UpdaterBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun sharedExactCborRequestsAndResultsMatchIndependentGoldens() {
    val names = FileNames
    for (name in names) {
      val bytes = UpdaterHostLibrary.golden("${name}_request")
      val request = rustCbor.decodeFromByteArray<UpdateRequest>(bytes)
      assertContentEquals(bytes, rustCbor.encodeToByteArray(request), name)
      val response = UpdaterHostLibrary.golden("${name}_response")
      val expected = rustCbor.decodeFromByteArray<ApiEnvelope<UpdatePlan>>(response)
      var calls = 0
      val native: UpdateNativeCall = { input, size, output ->
        calls++
        assertContentEquals(bytes, ByteArray(size).also { input.duplicate().get(it) })
        output.put(response)
        response.size
      }
      if (request.version != 1 || request.policy.version != 1) {
        assertFailsWith<IllegalArgumentException> { planUpdate(request, native) }
        assertEquals(0, calls)
      } else if (expected.error != null) {
        val failure = assertFailsWith<IOException> { planUpdate(request, native) }
        assertEquals(expected.error.message, failure.message)
        assertEquals(1, calls)
        assertEquals(
          expected.error.message,
          assertFailsWith<IOException> { planUpdate(request) }.message,
        )
      } else {
        assertEquals(expected.payload, planUpdate(request, native), name)
        assertEquals(1, calls)
        assertEquals(expected.payload, planUpdate(request), name)
      }
    }
    assertEquals(27, names.size)
  }

  @Test
  fun clockMatchesFrozenKotlinInstantSaturationNanosForceAndSignedIntervals() {
    for (last in
      listOf(
        Long.MIN_VALUE,
        -31_557_014_167_219_200L,
        -1L,
        0L,
        31_556_889_864_403_199L,
        Long.MAX_VALUE,
      )) {
      for (interval in listOf(Int.MIN_VALUE, -1, 0, 1, Int.MAX_VALUE)) {
        for (now in
          listOf(
            Instant.fromEpochSeconds(Long.MIN_VALUE),
            Instant.fromEpochSeconds(-1),
            Instant.fromEpochSeconds(86_400),
            Instant.fromEpochSeconds(86_400, 1),
            Instant.fromEpochSeconds(Long.MAX_VALUE),
          )) {
          for (force in listOf(false, true)) {
            // Characterization only; production no longer computes this rule in Kotlin.
            val expected =
              force || interval != 0 && now > Instant.fromEpochSeconds(last) + interval.days
            val actual =
              planUpdatePolicy(
                UpdatePolicyRequest(
                  operation = UpdateOperation.CHECK,
                  nowSeconds = now.epochSeconds,
                  nowNanos = now.nanosecondsOfSecond,
                  lastSeconds = last,
                  intervalDays = interval,
                  forceCheck = force,
                )
              )
            assertEquals(
              expected,
              actual.decision == UpdateDecision.CHECK,
              "$last/$interval/$now/$force",
            )
          }
        }
      }
    }
  }

  @Test
  fun abiUsesOnlyFirstEntryUnknownMapsToUniversalAndFlavorIsCaseSensitive() {
    for (abi in listOf("arm64-v8a", "x86_64", "armeabi-v7a", "x86", "unknown")) {
      val expectedAbi = if (abi in setOf("arm64-v8a", "x86_64", "armeabi-v7a")) abi else "universal"
      val request =
        UpdatePolicyRequest(
          operation = UpdateOperation.MATCH,
          flavor = "default",
          abis = listOf(abi, "arm64-v8a"),
          names = listOf("prefix-default-$expectedAbi.apk"),
        )
      assertEquals(true, planUpdatePolicy(request).matched)
      assertEquals(false, planUpdatePolicy(request.copy(flavor = "Default")).matched)
      assertEquals(true, planUpdatePolicy(request.copy(flavor = "")).matched)
    }
  }

  @Test
  fun versionAndShortShaCharacterizationIncludesSplitSurrogateAndEmptyStrings() {
    for ((current, latest) in listOf("" to "", "v2" to "v1", "1" to "v1", "1" to "1")) {
      val plan =
        planUpdatePolicy(
          UpdatePolicyRequest(
            operation = UpdateOperation.COMPARE_RELEASE,
            current = current,
            latest = latest,
          )
        )
      assertEquals(latest, plan.version)
      assertEquals(current != latest, plan.decision == UpdateDecision.CANDIDATE)
    }
    for ((current, latest) in
      listOf(
        "" to "",
        "abc" to "abc",
        "abcdef0" to "abcdef012345",
        "abcdef01" to "abcdef012345",
        "x" to "123456😀",
      )) {
      val plan =
        planUpdatePolicy(
          UpdatePolicyRequest(
            operation = UpdateOperation.COMPARE_WORKFLOW,
            current = current,
            latest = latest,
          )
        )
      assertEquals(latest.take(7), plan.version)
      assertEquals(current != latest.take(7), plan.decision == UpdateDecision.CANDIDATE)
    }
  }

  @Test
  fun invalidInputBudgetsRejectBeforeJniWithoutPayloadEcho() {
    val base = UpdatePolicyRequest(operation = UpdateOperation.CHECK)
    for (wrong in
      listOf(
        base.copy(version = 2),
        base.copy(nowNanos = -1),
        base.copy(nowNanos = 1_000_000_000),
        base.copy(nowSeconds = Long.MAX_VALUE),
        base.copy(names = List(UPDATE_MAX_ITEMS + 1) { "" }),
        base.copy(
          abis =
            List(33) {
              ""
            }
        ),
        base.copy(flavor = "secret".repeat(UPDATE_MAX_TEXT)),
        base.copy(names = listOf("one"), matches = listOf(false, true)),
      )) {
      val failure =
        assertFailsWith<IllegalArgumentException> {
          planUpdatePolicy(wrong) { _, _, _ -> fail("Invalid request entered JNI") }
        }
      assertFalse(failure.message.orEmpty().contains("secret"))
    }
    assertFailsWith<IllegalArgumentException> {
      planUpdate(
        UpdateRequest(
          operation = UpdateResponseOperation.REPO,
          policy = base,
          json = "secret".repeat(UPDATE_MAX_JSON),
        )
      ) { _, _, _ ->
        fail("Oversized JSON entered JNI")
      }
    }
  }

  @Test
  fun wrongEnvelopeClosedDecisionBoundsAndUntrustedErrorAreRejectedBeforeEffects() {
    val request =
      rustCbor.decodeFromByteArray<UpdateRequest>(
        UpdaterHostLibrary.golden("select_release_request")
      )
    val plan =
      rustCbor
        .decodeFromByteArray<ApiEnvelope<UpdatePlan>>(
          UpdaterHostLibrary.golden("select_release_response")
        )
        .payload!!
    for (envelope in
      listOf(
        ApiEnvelope(2, "updater", plan),
        ApiEnvelope(1, "other", plan),
      )) {
      assertFailsWith<IllegalArgumentException> { planUpdate(request, respond(envelope)) }
    }
    for (envelope in
      listOf(
        UncheckedEnvelope(
          1,
          "updater",
          plan,
          RustError(RustErrorKind.ERROR, "Invalid update response"),
        ),
        UncheckedEnvelope(1, "updater", null, null),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planUpdate(request) { _, _, output ->
          val bytes = rustCbor.encodeToByteArray(envelope)
          output.put(bytes)
          bytes.size
        }
      }
    }
    val policy = checkNotNull(plan.policy)
    for (wrong in
      listOf(
        plan.copy(policy = policy.copy(index = -1)),
        plan.copy(policy = policy.copy(index = 2)),
        plan.copy(policy = policy.copy(decision = UpdateDecision.CANDIDATE)),
        plan.copy(policy = policy.copy(order = listOf(0, 1))),
        plan.copy(downloadLink = "unexpected"),
        plan.copy(selectedIndex = 0, sourceCount = 0),
        plan.copy(policy = null),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planUpdate(request, respond(ApiEnvelope(1, "updater", wrong)))
      }
    }
    val failure =
      assertFailsWith<IllegalArgumentException> {
        planUpdate(
          request,
          respond(
            ApiEnvelope(1, "updater", error = RustError(RustErrorKind.ERROR, "secret-url-token"))
          ),
        )
      }
    assertFalse(failure.message.orEmpty().contains("secret"))
  }

  @Test
  fun returnedVersionUnitsShaAssociationAndRemoteIndicesAreStrict() {
    val request =
      rustCbor.decodeFromByteArray<UpdateRequest>(UpdaterHostLibrary.golden("workflow_request"))
    val plan =
      rustCbor
        .decodeFromByteArray<ApiEnvelope<UpdatePlan>>(
          UpdaterHostLibrary.golden("workflow_response")
        )
        .payload!!
    val policy = checkNotNull(plan.policy)
    for (wrong in
      listOf(
        plan.copy(selectedIndex = 1),
        plan.copy(sourceCount = UPDATE_MAX_ITEMS + 1),
        plan.copy(info = "stray"),
        plan.copy(policy = policy.copy(versionUnits = listOf(65536))),
        plan.copy(
          policy =
            policy.copy(
              versionUnits =
                List(8) {
                  97
                }
            )
        ),
        plan.copy(policy = policy.copy(versionUnits = listOf(97))),
        plan.copy(policy = policy.copy(decision = UpdateDecision.NO_UPDATE)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planUpdate(request, respond(ApiEnvelope(1, "updater", wrong)))
      }
    }
  }

  @Test
  fun nativeMalformedSizeCancellationAndLinkagePropagateWithoutRetry() {
    val request = UpdatePolicyRequest(operation = UpdateOperation.CHECK)
    for (failure in
      listOf(
        LinkageError("fixture"),
        CancellationException("fixture"),
        IllegalStateException("fixture"),
      )) {
      var calls = 0
      try {
        planUpdatePolicy(request) { _, _, _ ->
          calls++
          throw failure
        }
        fail("Failure hidden")
      } catch (actual: Throwable) {
        assertSame(failure, actual)
      }
      assertEquals(1, calls)
    }
    assertFailsWith<IllegalArgumentException> { planUpdatePolicy(request) { _, _, _ -> 0 } }
    assertFailsWith<IllegalArgumentException> {
      planUpdatePolicy(request) { _, _, _ -> Int.MAX_VALUE }
    }
    assertEquals(
      "Invalid update response",
      assertFailsWith<IOException> {
          planUpdatePolicy(request) { _, _, output ->
            output.put(0xff.toByte())
            1
          }
        }
        .message,
    )
    val selected =
      rustCbor.decodeFromByteArray<UpdateRequest>(
        UpdaterHostLibrary.golden("select_release_request")
      )
    val invalidEnum = UpdaterHostLibrary.golden("select_release_response")
    val offset = invalidEnum.toString(Charsets.ISO_8859_1).indexOf("selected")
    "secret!!".encodeToByteArray().copyInto(invalidEnum, offset)
    val failure =
      assertFailsWith<IOException> {
        planUpdate(selected) { _, _, output ->
          output.put(invalidEnum)
          invalidEnum.size
        }
      }
    assertEquals("Invalid update response", failure.message)
    assertNull(failure.cause)
  }

  private fun respond(envelope: ApiEnvelope<UpdatePlan>): UpdateNativeCall =
    { _, _, output: ByteBuffer ->
      val bytes = rustCbor.encodeToByteArray(envelope)
      output.put(bytes)
      bytes.size
    }

  @Serializable
  private data class UncheckedEnvelope(
    val version: Int,
    val module: String,
    val payload: UpdatePlan?,
    val error: RustError?,
  )

  private val FileNames =
    listOf(
      "skip",
      "force",
      "match",
      "prepare_release",
      "prepare_artifacts",
      "select_release",
      "select_artifacts",
      "select_custom",
      "select_workflow",
      "compare_release",
      "compare_workflow",
      "compare_split",
      "repo",
      "release",
      "release_same",
      "release_empty",
      "workflow",
      "workflow_same",
      "workflow_empty",
      "artifacts",
      "artifacts_empty",
      "comparison",
      "invalid_json",
      "unsupported",
      "invalid_policy_version",
      "invalid_matches",
      "release_no_match",
    )
}
