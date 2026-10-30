package com.hippo.ehviewer.bridge.gallery

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame

class CommentDisplayBridgeTest {
  @Test
  fun rawUtf16AndPlatformDigitsReachOneNativeCall() {
    val text = "\ud800😀 http://a.b:١2"
    var calls = 0
    val links =
      commentLinks(text) { units, facts ->
        calls++
        assertContentEquals(text.toCharArray(), units)
        assertContentEquals(commentDigitFacts(text), facts)
        intArrayOf(4, text.length)
      }
    assertEquals(listOf(CommentLink(4, text.length)), links)
    assertEquals(1, calls)
    assertEquals(1, commentDigitFacts("١")[0].toInt() and 15)
  }

  @Test
  fun invalidRangesNeverReachSpanEffects() {
    for (result in
      listOf(
        intArrayOf(1),
        intArrayOf(-1, 3),
        intArrayOf(2, 2),
        intArrayOf(0, 20),
        intArrayOf(0, 4, 3, 7),
      )) {
      assertFailsWith<IllegalArgumentException> { commentLinks("http://a.b") { _, _ -> result } }
    }
    assertFailsWith<IllegalArgumentException> {
      commentLinks("x".repeat(128 * 1024 + 1)) { _, _ -> error("called") }
    }
    assertEquals(emptyList(), commentLinks("none") { _, _ -> intArrayOf() })
  }

  @Test
  fun dimensionResultsAndNumberFailuresAreNotReinterpreted() {
    assertContentEquals(
      intArrayOf(100, 200, -11, -15),
      commentImageDimensions("a-b-100-200-c_-11") { _, _ -> intArrayOf(100, 200, -11, -15) },
    )
    assertEquals(null, commentImageDimensions("none") { _, _ -> null })
    assertFailsWith<IllegalArgumentException> {
      commentImageDimensions("none") { _, _ -> intArrayOf(1) }
    }
    val failure = NumberFormatException("fixture")
    assertSame(
      failure,
      assertFailsWith<NumberFormatException> {
        commentImageDimensions("fixture") { _, _ -> throw failure }
      },
    )
  }
}
