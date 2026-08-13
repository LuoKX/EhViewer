package com.hippo.ehviewer.jni

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

class GifUtilsTest {
  @Test
  fun selectsExactlyAndroidNineThroughThirteen() {
    assertFalse(shouldUseGifWorkaround(27))
    assertTrue(shouldUseGifWorkaround(28))
    assertTrue(shouldUseGifWorkaround(33))
    assertFalse(shouldUseGifWorkaround(34))
  }

  @Test
  fun ownedPreparationCallsTheNativeBoundaryExactlyOnceWithoutFallback() {
    val expected = byteArrayOf(1, 2, 3)
    var calls = 0
    var descriptor = -1
    val result =
      prepareGifSourceOwned(42) {
        calls++
        descriptor = it
        expected
      }

    assertEquals(1, calls)
    assertEquals(42, descriptor)
    assertContentEquals(expected, requireNotNull(result))

    assertNull(prepareGifSourceOwned(7) { null })
  }
}
