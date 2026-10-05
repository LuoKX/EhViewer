package com.hippo.ehviewer.gallery

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.runBlocking

/**
 * Budget arithmetic moved to Rust tests; these tests freeze Android effect/cancellation ordering.
 */
class ReaderMemoryPolicyTest {
  @Test
  fun successDoesNotTrimOrRetry() = runBlocking {
    var calls = 0
    var trims = 0
    assertEquals(
      "decoded",
      retryAfterImageCacheTrim(2, { trims++ }) {
        calls++
        "decoded"
      },
    )
    assertEquals(1, calls)
    assertEquals(0, trims)
  }

  @Test
  fun outOfMemoryTrimsBeforeExactlyOneRetry() = runBlocking {
    val events = mutableListOf<String>()
    var calls = 0
    assertEquals(
      "decoded",
      retryAfterImageCacheTrim(2, { events += "trim" }) {
        events += "decode"
        if (++calls == 1) throw OutOfMemoryError("simulated")
        "decoded"
      },
    )
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
        retryAfterImageCacheTrim(2, { trims++ }) { if (++calls == 1) throw first else throw second }
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
      assertSame(
        failure,
        assertFailsWith<Exception> {
          retryAfterImageCacheTrim(2, { trims++ }) {
            calls++
            throw failure
          }
        },
      )
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
        retryAfterImageCacheTrim(2, { job.cancel() }) {
          calls++
          throw OutOfMemoryError("simulated")
        }
      }
    }
    assertEquals(1, calls)
  }

  @Test
  fun oneAttemptAndInvalidBudgetsNeverTrim() = runBlocking {
    var trims = 0
    assertFailsWith<OutOfMemoryError> {
      retryAfterImageCacheTrim(1, { trims++ }) { throw OutOfMemoryError("single") }
    }
    for (budget in listOf(0, 3)) assertFailsWith<IllegalArgumentException> {
      retryAfterImageCacheTrim(budget, { trims++ }) { "not called" }
    }
    assertEquals(0, trims)
  }
}
