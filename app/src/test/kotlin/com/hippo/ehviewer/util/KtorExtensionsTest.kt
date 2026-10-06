package com.hippo.ehviewer.util

import io.ktor.utils.io.ByteChannel
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.writeFully
import java.io.IOException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield

class KtorExtensionsTest {
  @Test
  fun acceptsEmptyShortAndExactlyFullResponses() =
    runBlocking {
      for (size in listOf(0, 3, 4)) {
        for (buffer in listOf(ByteBuffer.allocate(4), ByteBuffer.allocateDirect(4))) {
          val bytes = ByteArray(size) { (it + 1).toByte() }
          ByteReadChannel(bytes).readToBuffer(buffer)

          assertEquals(size, buffer.position())
          buffer.flip()
          val actual = ByteArray(buffer.remaining())
          buffer.get(actual)
          assertContentEquals(bytes, actual)
        }
      }
    }

  @Test
  fun honorsBufferPositionAndLimit() =
    runBlocking {
      val buffer =
        ByteBuffer.allocate(8).apply {
          put(9.toByte())
          limit(5)
        }
      ByteReadChannel(byteArrayOf(1, 2, 3, 4)).readToBuffer(buffer)

      assertEquals(5, buffer.position())
      assertEquals(5, buffer.limit())
      assertContentEquals(byteArrayOf(9, 1, 2, 3, 4, 0, 0, 0), buffer.array())
    }

  @Test
  fun rejectsOversizedResponseInsteadOfReadingIntoFullBuffer() =
    runBlocking {
      val buffer = ByteBuffer.allocateDirect(4)
      val failure =
        assertFailsWith<IOException> {
          ByteReadChannel(ByteArray(5)).readToBuffer(buffer)
        }
      assertEquals("HTTP response exceeds buffer capacity", failure.message)
      assertEquals(4, buffer.position())
    }

  @Test
  fun allowsZeroCapacityOnlyForEmptyResponses() =
    runBlocking<Unit> {
      ByteReadChannel(ByteArray(0)).readToBuffer(ByteBuffer.allocate(0))
      assertFailsWith<IOException> {
        ByteReadChannel(byteArrayOf(1)).readToBuffer(ByteBuffer.allocate(0))
      }
    }

  @Test
  fun propagatesChannelFailureInsteadOfReturningTruncatedBody() =
    runBlocking {
      val failure = IOException("transport failed")
      val channel = ByteChannel().apply { cancel(failure) }

      val actual =
        assertFailsWith<IOException> {
          channel.readToBuffer(ByteBuffer.allocate(4))
        }
      assertEquals(failure.message, actual.message)
      assertSame(failure, actual.cause)
    }

  @Test
  fun waitsForEndOfResponseAtExactBufferBoundary() =
    runBlocking {
      val channel = ByteChannel(autoFlush = true)
      channel.writeFully(byteArrayOf(1, 2, 3, 4))
      val reader =
        async(start = CoroutineStart.UNDISPATCHED) {
          channel.readToBuffer(ByteBuffer.allocate(4))
        }
      assertFalse(reader.isCompleted)

      channel.close()
      reader.await()
    }

  @Test
  fun rejectsAdditionalBytesArrivingAfterBufferFills() =
    runBlocking {
      val channel = ByteChannel(autoFlush = true)
      channel.writeFully(byteArrayOf(1, 2, 3, 4))
      val reader =
        async(start = CoroutineStart.UNDISPATCHED) {
          runCatching { channel.readToBuffer(ByteBuffer.allocate(4)) }
        }
      assertFalse(reader.isCompleted)

      channel.writeFully(byteArrayOf(5))
      assertTrue(reader.await().exceptionOrNull() is IOException)
      channel.close()
    }

  @Test
  fun propagatesFailureAfterPartialResponse() =
    runBlocking {
      val channel = ByteChannel(autoFlush = true)
      val buffer = ByteBuffer.allocate(4)
      val reader =
        async(start = CoroutineStart.UNDISPATCHED) {
          runCatching { channel.readToBuffer(buffer) }
        }
      channel.writeFully(byteArrayOf(1, 2))
      yield()
      assertEquals(2, buffer.position())

      val failure = IOException("connection interrupted")
      channel.cancel(failure)
      val actual = assertFailsWith<IOException> { reader.await().getOrThrow() }
      assertEquals(failure.message, actual.message)
      // A suspended read may add another Ktor close wrapper; retain the original cause chain.
      assertTrue(generateSequence<Throwable>(actual) { it.cause }.any { it === failure })
    }

  @Test
  fun cancellationInterruptsBoundaryWait() =
    runBlocking {
      val channel = ByteChannel(autoFlush = true)
      channel.writeFully(byteArrayOf(1, 2, 3, 4))
      val reader =
        launch(start = CoroutineStart.UNDISPATCHED) {
          channel.readToBuffer(ByteBuffer.allocate(4))
        }

      reader.cancelAndJoin()
      assertTrue(reader.isCancelled)
      channel.cancel(null)
    }

  @Test
  fun observesCancellationEvenWhenAllBytesAreAlreadyBuffered() =
    runBlocking {
      val reader =
        launch(start = CoroutineStart.UNDISPATCHED) {
          currentCoroutineContext().cancel()
          assertFailsWith<CancellationException> {
            ByteReadChannel(byteArrayOf(1, 2, 3)).readToBuffer(ByteBuffer.allocate(4))
          }
        }
      reader.join()
      assertTrue(reader.isCancelled)
    }
}
