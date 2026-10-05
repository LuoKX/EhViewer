package com.hippo.ehviewer.bridge

import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.gallery.*
import com.hippo.ehviewer.bridge.reader.*
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class P2BridgeTest {
  @Test
  fun readerMemoryGoldenAndScalarSizingRouteExactlyOnce() {
    val request =
      rustCbor.decodeFromByteArray<ReaderPolicyRequest>(golden("reader_policy", "request"))
    var calls = 0
    val plan =
      planReaderPolicy(request) { buffer, size, output ->
        calls++
        assertEquals(
          request,
          rustCbor.decodeFromByteArray<ReaderPolicyRequest>(input(buffer, size)),
        )
        golden("reader_policy", "response").let {
          output.put(it)
          it.size
        }
      }
    assertEquals(1, calls)
    assertEquals(64 * 1024 * 1024, plan.cacheSize)
    assertEquals(2, plan.decodeConcurrency)
    assertEquals(2, plan.decodeAttempts)
    assertEquals(
      64,
      readerImageEntrySize(Long.MAX_VALUE, 64) { version, allocation, cache ->
        assertEquals(1, version)
        assertEquals(Long.MAX_VALUE, allocation)
        assertEquals(64, cache)
        64
      },
    )
    assertFailsWith<IllegalArgumentException> { readerImageEntrySize(1, 64) { _, _, _ -> 0 } }
  }

  @Test
  fun schedulerGoldenKeepsOrderedEvictionAndWait() {
    val request =
      rustCbor.decodeFromByteArray<RecoverySearchRequest>(golden("recovery_search", "request"))
    val plan = planRecoverySearch(request, direct(request, golden("recovery_search", "response")))
    assertEquals(1L, plan.waitMillis)
    assertEquals(1, plan.evictCount)
    assertTrue(plan.store)
    assertFalse(plan.useCache)
  }

  @Test
  fun localSelectionGoldenAndAllIndexValidation() {
    val request = rustCbor.decodeFromByteArray<LocalPagesRequest>(golden("local_pages", "request"))
    assertEquals(
      listOf(2, 0),
      selectLocalPages(request, direct(request, golden("local_pages", "response"))).map {
        it.sourceIndex
      },
    )
    for (pages in
      listOf(
        listOf(LocalPageSelection(-1, "jpg")),
        listOf(LocalPageSelection(99, "jpg")),
        listOf(LocalPageSelection(0, "JPG")),
        listOf(LocalPageSelection(0, "jpg"), LocalPageSelection(0, "jpg")),
      )) {
      assertFailsWith<IllegalArgumentException> {
        selectLocalPages(
          request,
          direct(request, rustCbor.encodeToByteArray(ApiEnvelope(1, "localPages", pages, null))),
        )
      }
    }
  }

  @Test
  fun fullSpiderInfoGoldenPreservesEveryTokenAndDecodeShape() {
    val request =
      rustCbor.decodeFromByteArray<SpiderInfoCodecRequest>(golden("spider_info", "request"))
    var calls = 0
    val plan =
      codecSpiderInfo(request) { buffer, size ->
        calls++
        assertEquals(
          request,
          rustCbor.decodeFromByteArray<SpiderInfoCodecRequest>(input(buffer, size)),
        )
        golden("spider_info", "response")
      }
    assertEquals(1, calls)
    val stored = rustCbor.decodeFromByteArray<SpiderInfoData>(checkNotNull(plan.bytes))
    assertEquals(mapOf(0 to "a", 2 to "b"), stored.pTokenMap)
    assertEquals(40, stored.previewPerPage)
    val decodeRequest =
      SpiderInfoCodecRequest(
        kind = SpiderInfoCodecKind.DECODE,
        bytes = plan.bytes,
        allowLegacy = false,
      )
    val envelope =
      ApiEnvelope(
        1,
        "spiderInfo",
        SpiderInfoCodecPlan(SpiderInfoCodecKind.DECODE, info = stored),
        null,
      )
    assertEquals(
      stored,
      codecSpiderInfo(decodeRequest) { _, _ -> rustCbor.encodeToByteArray(envelope) }.info,
    )
    assertFailsWith<IllegalArgumentException> {
      codecSpiderInfo(decodeRequest) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(
            1,
            "spiderInfo",
            SpiderInfoCodecPlan(SpiderInfoCodecKind.DECODE, info = stored, bytes = byteArrayOf(1)),
            null,
          )
        )
      }
    }
    assertFailsWith<IllegalArgumentException> {
      codecSpiderInfo(
        SpiderInfoCodecRequest(
          kind = SpiderInfoCodecKind.DECODE,
          bytes = ByteArray(MAX_SPIDER_INFO_BYTES + 1),
        )
      ) { _, _ ->
        error("must not invoke")
      }
    }
  }

  @Test
  fun cacheKeyGoldenKeepsTimedPreviewKey() {
    val request =
      rustCbor.decodeFromByteArray<CacheKeyRequest>(golden("cache_key", "request", net = true))
    assertEquals(
      "$" + "m/7-2",
      planCacheKey(request, direct(request, golden("cache_key", "response", net = true))),
    )
  }

  @Test
  fun wrongContractsVersionsFieldsAndNativeFailuresSurface() {
    val request =
      ReaderPolicyRequest(kind = ReaderPolicyKind.MEMORY, appMaxMemory = 1, totalMemory = 0)
    for (envelope in
      listOf(
        ApiEnvelope(2, "readerPolicy", ReaderPolicyPlan(ReaderPolicyKind.MEMORY, 1, 2, 2), null),
        ApiEnvelope(1, "wrong", ReaderPolicyPlan(ReaderPolicyKind.MEMORY, 1, 2, 2), null),
        ApiEnvelope(
          1,
          "readerPolicy",
          ReaderPolicyPlan(ReaderPolicyKind.QUOTA, exceeded = true),
          null,
        ),
        ApiEnvelope(1, "readerPolicy", ReaderPolicyPlan(ReaderPolicyKind.MEMORY, 1, 2, 3), null),
        ApiEnvelope(
          1,
          "readerPolicy",
          ReaderPolicyPlan(ReaderPolicyKind.MEMORY, 1, 2, 2, exceeded = true),
          null,
        ),
      )) assertFailsWith<IllegalArgumentException> {
      planReaderPolicy(request, direct(request, rustCbor.encodeToByteArray(envelope)))
    }
    val failure = UnsatisfiedLinkError("simulated")
    var calls = 0
    assertSame(
      failure,
      assertFailsWith<UnsatisfiedLinkError> {
        planReaderPolicy(request) { _, _, _ ->
          calls++
          throw failure
        }
      },
    )
    assertEquals(1, calls)
    assertFailsWith<RustFailureException> {
      planReaderPolicy(
        request,
        direct(
          request,
          rustCbor.encodeToByteArray(
            ApiEnvelope<ReaderPolicyPlan>(
              1,
              "readerPolicy",
              null,
              RustError(RustErrorKind.ERROR, "invalid"),
            )
          ),
        ),
      )
    }
  }

  @Test
  fun nativeTransferLifecycleAndSentinelAreClosed() {
    val operations = mutableListOf<Int>()
    var closes = 0
    val native =
      TransferSpeedNative(
        open = { version, window ->
          assertEquals(1, version)
          assertEquals(1_000L, window)
          7L
        },
        update = { handle, operation, _, _ ->
          assertEquals(7L, handle)
          operations += operation
          if (operation == 2) -1 else 0
        },
        close = {
          closes++
          assertEquals(7L, it)
        },
      )
    val handle = TransferSpeedHandle(1_000, native)
    handle.track(1, 2)
    assertNull(handle.sample(2))
    handle.reset()
    handle.close()
    handle.close()
    assertEquals(listOf(1, 2, 0), operations)
    assertEquals(1, closes)
    assertFailsWith<IllegalStateException> { handle.sample(3) }
    assertFailsWith<IllegalArgumentException> {
      TransferSpeedHandle(1, TransferSpeedNative(open = { _, _ -> 0 }))
    }
    val invalid =
      TransferSpeedHandle(
        1,
        TransferSpeedNative(open = { _, _ -> 1 }, update = { _, _, _, _ -> -2 }, close = {}),
      )
    assertFailsWith<IllegalArgumentException> { invalid.sample(0) }
    invalid.close()
  }

  private inline fun <reified T> direct(
    request: T,
    response: ByteArray,
  ): (ByteBuffer, Int, ByteBuffer) -> Int = { buffer, size, output ->
    assertEquals(request, rustCbor.decodeFromByteArray<T>(input(buffer, size)))
    output.put(response)
    response.size
  }

  private fun input(buffer: ByteBuffer, size: Int) =
    ByteArray(size).also { buffer.duplicate().get(it) }

  private fun golden(owner: String, kind: String, net: Boolean = false): ByteArray {
    val relative =
      "src/main/rust/crates/" +
        (if (net) "net" else "domain") +
        "/test_data/${owner}_${kind}.cbor.hex"
    val file = File(relative).takeIf(File::exists) ?: File("app/$relative")
    return file.readText().trim().chunked(2).map { it.toInt(16).toByte() }.toByteArray()
  }
}
