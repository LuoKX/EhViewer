package com.hippo.ehviewer.coil

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class AnimatedWebPBufferTest {
  @Test
  fun createsDirectViewsOfOnlyTheRemainingHeapAndDirectBytes() {
    listOf(ByteBuffer.allocate(6), ByteBuffer.allocateDirect(6)).forEach { source ->
      source.put(byteArrayOf(9, 1, 2, 3, 8, 7))
      source.position(1)
      source.limit(4)

      val result = source.asDirectRemainingBuffer()
      val bytes = ByteArray(result.remaining())
      result.get(bytes)

      assertTrue(result.isDirect)
      assertContentEquals(byteArrayOf(1, 2, 3), bytes)
      assertEquals(1, source.position())
      assertEquals(4, source.limit())
    }
  }
}
