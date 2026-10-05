package com.hippo.ehviewer.gallery

import androidx.collection.SieveCache
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking

class ReaderMemoryPolicyTest {
  @Test
  fun heapLimitWinsOverLargeDeviceMemory() {
    assertEquals(64 * MIB, readerImageCacheSize(256L * MIB, 8L * 1024 * MIB))
    assertEquals(32 * MIB, readerImageCacheSize(128L * MIB, 4L * 1024 * MIB))
  }

  @Test
  fun respectsDeviceBudgetAndAbsoluteCapWithoutIntegerOverflow() {
    assertEquals(16 * MIB, readerImageCacheSize(256L * MIB, 128L * MIB))
    assertEquals(128 * MIB, readerImageCacheSize(Long.MAX_VALUE, Long.MAX_VALUE))
  }

  @Test
  fun missingDeviceMemoryFallsBackToHeapBudget() {
    assertEquals(64 * MIB, readerImageCacheSize(256L * MIB, -1L))
    assertEquals(64 * MIB, readerImageCacheSize(256L * MIB, 0L))
  }

  @Test
  fun cacheBudgetIsAlwaysPositive() {
    assertEquals(1, readerImageCacheSize(1L, 1L))
    assertEquals(1, readerImageCacheSize(0L, -1L))
  }

  @Test
  fun entrySizeSaturatesWithoutIntegerOverflow() {
    assertEquals(64 * MIB, readerImageCacheEntrySize(Long.MAX_VALUE, 64 * MIB))
    assertEquals(1, readerImageCacheEntrySize(0L, 64 * MIB))
    assertEquals(42, readerImageCacheEntrySize(42L, 64 * MIB))
  }

  @Test
  fun oversizedPageSurvivesInsertionAndOccupiesWholeCache() {
    val removed = mutableListOf<Int>()
    val cache =
      SieveCache<Int, Long>(
        maxSize = 64,
        sizeOf = { _, allocation -> readerImageCacheEntrySize(allocation, 64) },
        onEntryRemoved = { key, _, _, _ -> removed += key },
      )
    cache[1] = Long.MAX_VALUE
    assertEquals(64, cache.size)
    assertEquals(emptyList(), removed)
    cache[2] = 8L
    assertEquals(listOf(1), removed)
    assertEquals(8L, cache[2])
  }

  @Test
  fun successDoesNotTrimOrRetry() = runBlocking {
    var calls = 0
    var trims = 0
    val result =
      retryAfterImageCacheTrim({ trims++ }) {
        calls++
        "decoded"
      }
    assertEquals("decoded", result)
    assertEquals(1, calls)
    assertEquals(0, trims)
  }

  @Test
  fun outOfMemoryTrimsBeforeExactlyOneRetry() = runBlocking {
    val events = mutableListOf<String>()
    var calls = 0
    val result =
      retryAfterImageCacheTrim({ events += "trim" }) {
        events += "decode"
        if (++calls == 1) throw OutOfMemoryError("simulated")
        "decoded"
      }
    assertEquals("decoded", result)
    assertEquals(listOf("decode", "trim", "decode"), events)
  }

  @Test
  fun secondOutOfMemoryPropagatesWithoutAnotherTrim() = runBlocking {
    val first = OutOfMemoryError("first")
    val second = OutOfMemoryError("second")
    var calls = 0
    var trims = 0
    val failure =
      assertFailsWith<OutOfMemoryError> {
        retryAfterImageCacheTrim({ trims++ }) {
          if (++calls == 1) throw first else throw second
        }
      }
    assertSame(second, failure)
    assertEquals(2, calls)
    assertEquals(1, trims)
  }

  @Test
  fun cancellationAndOrdinaryFailuresAreNotRetried() = runBlocking {
    for (failure in listOf(CancellationException("cancelled"), IllegalStateException("failed"))) {
      var calls = 0
      var trims = 0
      val actual =
        assertFailsWith<Exception> {
          retryAfterImageCacheTrim({ trims++ }) {
            calls++
            throw failure
          }
        }
      assertSame(failure, actual)
      assertEquals(1, calls)
      assertEquals(0, trims)
    }
  }

  @Test
  fun cancellationDuringTrimPreventsRetry() {
    val job = Job()
    var calls = 0
    assertFailsWith<CancellationException> {
      runBlocking(job) {
        retryAfterImageCacheTrim({ job.cancel() }) {
          calls++
          throw OutOfMemoryError("simulated")
        }
      }
    }
    assertEquals(1, calls)
  }

  private companion object {
    const val MIB = 1024 * 1024
  }
}
