package com.hippo.ehviewer.bridge.files

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame

class DownloadFilesBridgeTest {
  @Test
  fun primitiveNameBoundaryPreservesRawUnitsAndLocaleFact() {
    var calls = 0
    val extension = "\ud800.tmp"
    val expected = "٠٠٠٠٠٠٠١.$extension"
    assertEquals(
      expected,
      rustPageFilename(0, extension, '٠') { index, zero, raw ->
        calls++
        assertEquals(0, index)
        assertEquals('٠'.code, zero)
        assertContentEquals(extension.toCharArray(), raw)
        expected.toCharArray()
      },
    )
    assertEquals(1, calls)
    assertFailsWith<IllegalArgumentException> {
      rustPageFilename(0, "jpg", '0') { _, _, _ -> "unsafe".toCharArray() }
    }
    assertFailsWith<IllegalArgumentException> {
      rustPageFilename(0, "x".repeat(4097), '0') { _, _, _ -> error("called") }
    }
  }

  @Test
  fun hashBoundaryDoesNotNormalizeUrlOrHideFailure() {
    val url = "\ud800/h/" + "a".repeat(40)
    assertEquals(
      "a".repeat(40),
      expectedImageHash(url) { raw ->
        assertContentEquals(url.toCharArray(), raw)
        "a".repeat(40)
      },
    )
    assertEquals(null, expectedImageHash("none") { null })
    assertFailsWith<IllegalArgumentException> { expectedImageHash(url) { "A".repeat(40) } }
    assertFailsWith<IllegalArgumentException> {
      expectedImageHash("x".repeat(16385)) { error("called") }
    }
    val failure = UnsatisfiedLinkError("fixture")
    assertSame(
      failure,
      assertFailsWith<UnsatisfiedLinkError> { expectedImageHash(url) { throw failure } },
    )
  }

  @Test
  fun rejectsIncompleteDuplicateUnorderedAndUnauthorizedDeletionSlots() {
    val facts = booleanArrayOf(true, false, true)
    assertContentEquals(intArrayOf(0, 2), planPostArchivePages(facts) { intArrayOf(0, 2) })
    for (indices in
      listOf(
        intArrayOf(),
        intArrayOf(0),
        intArrayOf(0, 0),
        intArrayOf(2, 0),
        intArrayOf(0, 1),
        intArrayOf(-1, 2),
        intArrayOf(0, 3),
      )) {
      assertFailsWith<IllegalArgumentException> { planPostArchivePages(facts) { indices } }
    }
    assertFailsWith<IllegalArgumentException> {
      planPostArchivePages(BooleanArray(100001)) { error("called") }
    }
  }
}
