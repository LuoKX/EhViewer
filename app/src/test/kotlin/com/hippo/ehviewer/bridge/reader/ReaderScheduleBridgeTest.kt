package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class ReaderScheduleBridgeTest {
  @Test
  fun encodesTheRustGeneratedRequestGoldenFixture() {
    val request =
      ReaderScheduleRequest(
        version = 1,
        snapshot =
          ReaderScheduleSnapshot(
            pageStates = listOf(ReaderPageState.READY, ReaderPageState.QUEUED),
            previousIndex = 0,
          ),
        command = ReaderScheduleCommand.request(index = 1, cached = false, prefetchPageCount = 3),
      )

    val encoded = rustCbor.encodeToByteArray(request)
    val encodedHex =
      encoded.joinToString(separator = "") { it.toUByte().toString(16).padStart(2, '0') }

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encodedHex)
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedResponseGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<ReaderSchedulePlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )
    val snapshot =
      ReaderScheduleSnapshot(
        pageStates = listOf(ReaderPageState.READY, ReaderPageState.READY, ReaderPageState.LOADING)
      )
    val command = ReaderScheduleCommand.request(index = 1, cached = true, prefetchPageCount = 1)

    val plan = envelope.toReaderSchedulePlan(snapshot, command)

    assertEquals(
      listOf(
        ReaderScheduleActionKind.SET_PREVIOUS,
        ReaderScheduleActionKind.USE_CACHED,
        ReaderScheduleActionKind.PREFETCH,
      ),
      plan.actions.map(ReaderScheduleAction::kind),
    )
    assertEquals(-3, plan.actions.last().boundsStart)
    assertEquals(7, plan.actions.last().boundsEnd)
  }

  @Test
  fun invokesTheSelectedNativeBoundaryAndValidatesTheCompletePlan() {
    val snapshot =
      ReaderScheduleSnapshot(
        pageStates =
          listOf(
            ReaderPageState.READY,
            ReaderPageState.READY,
            ReaderPageState.QUEUED,
            ReaderPageState.ERROR,
          ),
        previousIndex = 0,
      )
    val command = ReaderScheduleCommand.request(index = 1, cached = false, prefetchPageCount = 2)
    val expectedPlan =
      ReaderSchedulePlan(
        actions =
          listOf(
            indexAction(ReaderScheduleActionKind.SET_PREVIOUS, 1),
            indexAction(ReaderScheduleActionKind.RESET_ONE, 1),
            fetchAction(index = 1, force = false, originalImage = false),
            prefetchAction(pages = listOf(2, 3), boundsStart = -3, boundsEnd = 8),
          )
      )
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(version = 1, module = "readerSchedule", payload = expectedPlan)
      )
    var invocationCount = 0

    val actualPlan =
      planReaderSchedule(snapshot, command) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        val decoded = rustCbor.decodeFromByteArray<ReaderScheduleRequest>(requestBytes)
        assertEquals(snapshot, decoded.snapshot)
        assertEquals(command, decoded.command)
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(expectedPlan, actualPlan)
    assertFailsWith<IllegalArgumentException> {
      planReaderSchedule(snapshot, command) { _, _, _ -> 0 }
    }
    assertFailsWith<IllegalArgumentException> {
      planReaderSchedule(snapshot, command) { _, _, responseBuffer ->
        responseBuffer.capacity() + 1
      }
    }
  }

  @Test
  fun validatesRetryAndRestartActionOrder() {
    val snapshot =
      ReaderScheduleSnapshot(
        pageStates = listOf(ReaderPageState.ERROR, ReaderPageState.READY),
        previousIndex = 1,
      )
    val retryCommand = ReaderScheduleCommand.retry(index = 0, originalImage = true)
    val retryPlan =
      ReaderSchedulePlan(
        listOf(
          indexAction(ReaderScheduleActionKind.RESET_ONE, 0),
          indexAction(ReaderScheduleActionKind.REMOVE_CACHE, 0),
          fetchAction(index = 0, force = true, originalImage = true),
        )
      )
    assertEquals(
      retryPlan,
      ApiEnvelope(version = 1, module = "readerSchedule", payload = retryPlan)
        .toReaderSchedulePlan(snapshot, retryCommand),
    )

    val restartPlan =
      ReaderSchedulePlan(
        listOf(
          ReaderScheduleAction(ReaderScheduleActionKind.CLEAR_CACHE),
          ReaderScheduleAction(ReaderScheduleActionKind.RESET_ALL),
        )
      )
    assertEquals(
      restartPlan,
      ApiEnvelope(version = 1, module = "readerSchedule", payload = restartPlan)
        .toReaderSchedulePlan(snapshot, ReaderScheduleCommand.restart()),
    )
  }

  @Test
  fun rejectsWrongEnvelopeAndPreservesTypedRustFailure() {
    val snapshot = ReaderScheduleSnapshot(listOf(ReaderPageState.QUEUED))
    val command = ReaderScheduleCommand.restart()
    val plan =
      ReaderSchedulePlan(
        listOf(
          ReaderScheduleAction(ReaderScheduleActionKind.CLEAR_CACHE),
          ReaderScheduleAction(ReaderScheduleActionKind.RESET_ALL),
        )
      )

    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "readerSchedule", payload = plan)
        .toReaderSchedulePlan(snapshot, command)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "readerPage", payload = plan)
        .toReaderSchedulePlan(snapshot, command)
    }

    val failure = RustError(RustErrorKind.ERROR, "invalid reader snapshot")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<ReaderSchedulePlan>(version = 1, module = "readerSchedule", error = failure)
          .toReaderSchedulePlan(snapshot, command)
      }
    assertEquals(failure, error.failure)
  }

  @Test
  fun commandFactoriesAndSnapshotsRejectInvalidInputs() {
    assertEquals(ReaderScheduleCommandKind.REQUEST, ReaderScheduleCommand.request(0, false, 5).kind)
    assertEquals(true, ReaderScheduleCommand.retry(1, true).originalImage)
    assertEquals(ReaderScheduleCommandKind.RESTART, ReaderScheduleCommand.restart().kind)

    assertFailsWith<IllegalArgumentException> { ReaderScheduleSnapshot(emptyList()) }
    assertFailsWith<IllegalArgumentException> {
      ReaderScheduleSnapshot(listOf(ReaderPageState.QUEUED), previousIndex = 1)
    }
    assertFailsWith<IllegalArgumentException> { ReaderScheduleCommand.request(-1, false, 1) }
    assertFailsWith<IllegalArgumentException> { ReaderScheduleCommand.request(0, false, -1) }
    assertFailsWith<IllegalArgumentException> { ReaderScheduleCommand.request(0, false, 1_025) }
  }

  @Test
  fun rejectsMalformedActionShapesSequencesTargetsAndPrefetchWindows() {
    assertFailsWith<IllegalArgumentException> {
      ReaderScheduleAction(kind = ReaderScheduleActionKind.PREFETCH, pages = emptyList())
    }
    assertFailsWith<IllegalArgumentException> {
      ReaderScheduleAction(kind = ReaderScheduleActionKind.RESET_ALL, index = 0)
    }

    val snapshot =
      ReaderScheduleSnapshot(
        pageStates =
          listOf(
            ReaderPageState.READY,
            ReaderPageState.READY,
            ReaderPageState.QUEUED,
            ReaderPageState.ERROR,
          ),
        previousIndex = 0,
      )
    val command = ReaderScheduleCommand.request(index = 1, cached = false, prefetchPageCount = 2)
    val valid =
      ReaderSchedulePlan(
        listOf(
          indexAction(ReaderScheduleActionKind.SET_PREVIOUS, 1),
          indexAction(ReaderScheduleActionKind.RESET_ONE, 1),
          fetchAction(index = 1, force = false, originalImage = false),
          prefetchAction(listOf(2, 3), -3, 8),
        )
      )

    assertInvalid(snapshot, command, valid.copy(actions = valid.actions.reversed()))
    assertInvalid(
      snapshot,
      command,
      valid.copy(
        actions =
          valid.actions.map {
            if (it.kind == ReaderScheduleActionKind.RESET_ONE) indexAction(it.kind, 0) else it
          }
      ),
    )
    assertInvalid(
      snapshot,
      command,
      valid.copy(
        actions =
          valid.actions.map {
            if (it.kind == ReaderScheduleActionKind.FETCH) fetchAction(1, true, false) else it
          }
      ),
    )
    assertInvalid(
      snapshot,
      command,
      valid.copy(actions = valid.actions.dropLast(1) + prefetchAction(listOf(3, 2), -3, 8)),
    )
    assertInvalid(
      snapshot,
      command,
      valid.copy(actions = valid.actions.dropLast(1) + prefetchAction(listOf(2), -3, 8)),
    )
    assertInvalid(
      snapshot,
      command,
      valid.copy(actions = valid.actions.dropLast(1) + prefetchAction(listOf(2, 3), -4, 8)),
    )

    val retryOutOfBounds =
      ReaderSchedulePlan(
        listOf(
          indexAction(ReaderScheduleActionKind.RESET_ONE, 4),
          indexAction(ReaderScheduleActionKind.REMOVE_CACHE, 4),
          fetchAction(4, true, false),
        )
      )
    assertInvalid(snapshot, ReaderScheduleCommand.retry(4, false), retryOutOfBounds)

    val restartWithExtraAction =
      ReaderSchedulePlan(
        listOf(
          ReaderScheduleAction(ReaderScheduleActionKind.CLEAR_CACHE),
          ReaderScheduleAction(ReaderScheduleActionKind.RESET_ALL),
          ReaderScheduleAction(ReaderScheduleActionKind.RESET_ALL),
        )
      )
    assertInvalid(snapshot, ReaderScheduleCommand.restart(), restartWithExtraAction)
  }

  private fun assertInvalid(
    snapshot: ReaderScheduleSnapshot,
    command: ReaderScheduleCommand,
    plan: ReaderSchedulePlan,
  ) {
    val error =
      assertFailsWith<IllegalArgumentException> {
        ApiEnvelope(version = 1, module = "readerSchedule", payload = plan)
          .toReaderSchedulePlan(snapshot, command)
      }
    assertTrue(error.message.orEmpty().isNotBlank())
  }

  private fun indexAction(kind: ReaderScheduleActionKind, index: Int) =
    ReaderScheduleAction(kind = kind, index = index)

  private fun fetchAction(index: Int, force: Boolean, originalImage: Boolean) =
    ReaderScheduleAction(
      kind = ReaderScheduleActionKind.FETCH,
      index = index,
      force = force,
      originalImage = originalImage,
    )

  private fun prefetchAction(pages: List<Int>, boundsStart: Int, boundsEnd: Int) =
    ReaderScheduleAction(
      kind = ReaderScheduleActionKind.PREFETCH,
      pages = pages,
      boundsStart = boundsStart,
      boundsEnd = boundsEnd,
    )

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a36776657273696f6e0168736e617073686f74a26a706167655374617465738265726561647966" +
        "7175657565646d70726576696f7573496e6465780067636f6d6d616e64a4646b696e6467726571" +
        "7565737465696e6465780166636163686564f471707265666574636850616765436f756e7403"

    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e7265616465725363686564756c65677061796c6f" +
        "6164a167616374696f6e7383a2646b696e646b73657450726576696f757365696e64657801a264" +
        "6b696e646975736543616368656465696e64657801a4646b696e64687072656665746368657061" +
        "676573806b626f756e647353746172742269626f756e6473456e6407656572726f72f6"
  }
}
