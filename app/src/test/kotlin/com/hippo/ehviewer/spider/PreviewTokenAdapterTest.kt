package com.hippo.ehviewer.spider

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.reader.PreviewToken
import com.hippo.ehviewer.bridge.reader.PreviewTokenKind
import com.hippo.ehviewer.bridge.reader.PreviewTokenPlan
import com.hippo.ehviewer.bridge.reader.PreviewTokenRequest
import com.hippo.ehviewer.bridge.rustCbor
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class PreviewTokenAdapterTest {
  private val rows =
    listOf(
      PreviewToken(40, "first"),
      PreviewToken(40, ""),
      PreviewToken(-1, "negative"),
      PreviewToken(40, "last"),
    )
  private val apply =
    PreviewTokenPlan(PreviewTokenKind.APPLY, previewPerPage = 20, updates = listOf(0, 2, 3))

  @Test
  fun appliesOrderedSparseDeltasAgainstCurrentMapWithoutSnapshotOrAgeSuppression() {
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(40 to "old", 99 to "retained"))
    var calls = 0
    info.applyPreviews(rows, 2) { buffer, size ->
      calls++
      val input =
        rustCbor.decodeFromByteArray<PreviewTokenRequest>(
          ByteArray(size).also {
            buffer.get(it)
          }
        )
      assertEquals(rows, input.previews)
      assertEquals(null, input.index)
      // Current map changes after request formation; sparse delta preserves unrelated rows.
      info.pTokenMap[100] = "current"
      info.pTokenMap[40] = "newer"
      response(apply)
    }
    assertEquals(1, calls)
    assertEquals(20, info.previewPerPage)
    assertEquals(
      mapOf(40 to "last", -1 to "negative", 99 to "retained", 100 to "current"),
      info.pTokenMap,
    )
  }

  @Test
  fun validatesWholePlanBeforeSizeOrAnyMapMutation() {
    for (bad in
      listOf(
        apply.copy(updates = listOf(0, 3, 2)),
        apply.copy(updates = listOf(0, 2, 4)),
        apply.copy(previewPerPage = 19),
      )) {
      val info = SpiderInfo(1, "gallery", 100, hashMapOf(40 to "old"), previewPerPage = 30)
      assertFailsWith<IllegalArgumentException> {
        info.applyPreviews(
          rows,
          2,
        ) { _, _ ->
          response(bad)
        }
      }
      assertEquals(30, info.previewPerPage)
      assertEquals(mapOf(40 to "old"), info.pTokenMap)
    }
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(40 to "old"), previewPerPage = 30)
    assertFailsWith<RustFailureException> {
      info.applyPreviews(
        emptyList(),
        1,
      ) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope<PreviewTokenPlan>(
            1,
            "previewTokens",
            error = RustError(RustErrorKind.ERROR, "Empty nonzero preview page"),
          )
        )
      }
    }
    assertEquals(30, info.previewPerPage)
    assertEquals(mapOf(40 to "old"), info.pTokenMap)
  }

  @Test
  fun emptyFirstPageZeroSizeAndEmptyTokenLeaveExistingRowsUntouched() {
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(40 to "old"))
    info.applyPreviews(
      emptyList(),
      0,
    ) { _, _ ->
      response(PreviewTokenPlan(PreviewTokenKind.APPLY, previewPerPage = 0, updates = emptyList()))
    }
    assertEquals(0, info.previewPerPage)
    assertEquals(
      0,
      info.previewIndex(99) { _, _ ->
        response(PreviewTokenPlan(PreviewTokenKind.LOOKUP, previewIndex = 0))
      },
    )
    info.applyPreviews(
      listOf(PreviewToken(40, "")),
      0,
    ) { _, _ ->
      response(PreviewTokenPlan(PreviewTokenKind.APPLY, previewPerPage = 1, updates = emptyList()))
    }
    assertEquals(mapOf(40 to "old"), info.pTokenMap)
  }

  @Test
  fun snapshotsSourceFactsButNeverSnapshotsDestinationMap() {
    val info = SpiderInfo(1, "gallery", 100)
    val source = mutableListOf(PreviewToken(0, "original"))
    info.applyPreviews(source, 0) { _, _ ->
      source[0] = PreviewToken(9, "replacement")
      response(PreviewTokenPlan(PreviewTokenKind.APPLY, previewPerPage = 1, updates = listOf(0)))
    }
    assertEquals(mapOf(0 to "original"), info.pTokenMap)
  }

  @Test
  fun currentPreviewSizeDelegatesBeforeHttpWithoutMapTransport() {
    val info = SpiderInfo(1, "gallery", 100, hashMapOf(0 to "must-not-send"), previewPerPage = 20)
    assertEquals(
      2,
      info.previewIndex(41) { buffer, size ->
        val bytes = ByteArray(size).also { buffer.get(it) }
        assertFalse(bytes.toString(Charsets.ISO_8859_1).contains("must-not-send"))
        assertEquals(
          PreviewTokenRequest(kind = PreviewTokenKind.LOOKUP, index = 41, previewPerPage = 20),
          rustCbor.decodeFromByteArray<PreviewTokenRequest>(bytes),
        )
        response(PreviewTokenPlan(PreviewTokenKind.LOOKUP, previewIndex = 2))
      },
    )
    assertEquals(mapOf(0 to "must-not-send"), info.pTokenMap)
    assertEquals(20, info.previewPerPage)
  }

  @Test
  fun callerCoroutineLockRemainsOwnedAndSerializesLookupAndResponse() = runBlocking {
    val info = SpiderInfo(1, "gallery", 100, previewPerPage = 20)
    val lock = Mutex()
    val firstOwner = Any()
    val secondOwner = Any()
    lock.withLock(firstOwner) {
      val waiting =
        launch(start = CoroutineStart.UNDISPATCHED) {
          lock.withLock(secondOwner) {
            assertEquals(
              2,
              info.previewIndex(41) { _, _ ->
                assertTrue(lock.holdsLock(secondOwner))
                response(PreviewTokenPlan(PreviewTokenKind.LOOKUP, previewIndex = 2))
              },
            )
          }
        }
      assertFalse(waiting.isCompleted)
      info.applyPreviews(rows, 2) { _, _ ->
        assertTrue(lock.holdsLock(firstOwner))
        response(apply)
      }
      assertTrue(lock.holdsLock(firstOwner))
    }
  }

  @Test
  fun synchronizedMapOwnershipRemainsRespectedDuringDeltaWrites() {
    val info = SpiderInfo(1, "gallery", 100)
    val ready = CountDownLatch(1)
    val done = CountDownLatch(1)
    val worker: Thread
    synchronized(info.pTokenMap) {
      worker = thread {
        try {
          info.applyPreviews(rows, 2) { _, _ ->
            ready.countDown()
            response(apply)
          }
        } finally {
          done.countDown()
        }
      }
      assertTrue(ready.await(10, TimeUnit.SECONDS))
      assertFalse(done.await(100, TimeUnit.MILLISECONDS))
      assertTrue(info.pTokenMap.isEmpty())
    }
    assertTrue(done.await(10, TimeUnit.SECONDS))
    worker.join()
    assertEquals(mapOf(40 to "last", -1 to "negative"), info.pTokenMap)
  }

  private fun response(plan: PreviewTokenPlan) =
    rustCbor.encodeToByteArray(ApiEnvelope(1, "previewTokens", payload = plan))
}
