package com.hippo.ehviewer.image

import java.nio.ByteBuffer
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame

class ImageSourceTest {
  @Test
  fun openSourceReturnsOriginalBuffer() {
    val buffer = ByteBuffer.allocateDirect(16)
    val source = byteBufferSource(buffer) {}
    assertSame(buffer, source.source)
    source.close()
  }

  @Test
  fun closeDropsBufferAndReleasesExactlyOnce() {
    for (buffer in listOf(ByteBuffer.allocate(16), ByteBuffer.allocateDirect(16))) {
      var releases = 0
      val source = byteBufferSource(buffer) { releases++ }
      source.close()
      source.close()
      assertEquals(1, releases)
      val failure = assertFailsWith<IllegalStateException> { source.source }
      assertEquals("Image source is closed", failure.message)
    }
  }

  @Test
  fun concurrentCloseReleasesExactlyOnce() {
    val releases = AtomicInteger()
    val start = CountDownLatch(1)
    val source = byteBufferSource(ByteBuffer.allocateDirect(16)) { releases.incrementAndGet() }
    val threads =
      List(8) {
        thread {
          start.await()
          source.close()
        }
      }
    start.countDown()
    threads.forEach(Thread::join)
    assertEquals(1, releases.get())
    assertFailsWith<IllegalStateException> { source.source }
  }

  @Test
  fun bufferIsDroppedBeforeReleaseCallback() {
    lateinit var source: ByteBufferSource
    source =
      byteBufferSource(ByteBuffer.allocateDirect(16)) {
        assertFailsWith<IllegalStateException> { source.source }
      }
    source.close()
  }

  @Test
  fun failedReleaseStillDropsBufferAndIsNotRepeated() {
    val failure = IllegalStateException("release failed")
    var releases = 0
    val source =
      byteBufferSource(ByteBuffer.allocateDirect(16)) {
        releases++
        throw failure
      }
    assertSame(failure, assertFailsWith<IllegalStateException> { source.close() })
    assertFailsWith<IllegalStateException> { source.source }
    source.close()
    assertEquals(1, releases)
  }
}
