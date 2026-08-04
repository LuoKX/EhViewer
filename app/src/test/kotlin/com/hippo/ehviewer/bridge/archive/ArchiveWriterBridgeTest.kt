package com.hippo.ehviewer.bridge.archive

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ArchiveWriterBridgeTest {
  @Test
  fun rolloutExecutesExactlyOneSelectedWriter() {
    var rustCalls = 0
    var legacyCalls = 0
    val rustWrite = { rustCalls += 1 }
    val legacyWrite = { legacyCalls += 1 }

    writeGeneratedCbzWithSelectedWriter(true, rustWrite, legacyWrite)
    assertEquals(1, rustCalls)
    assertEquals(0, legacyCalls)

    writeGeneratedCbzWithSelectedWriter(false, rustWrite, legacyWrite)
    assertEquals(1, rustCalls)
    assertEquals(1, legacyCalls)
  }

  @Test
  fun rolloutNeverFallsBackAfterRustFailure() {
    var legacyCalls = 0
    assertFailsWith<IllegalStateException> {
      writeGeneratedCbzWithSelectedWriter(
        useRustWriter = true,
        rustWrite = { error("injected Rust failure") },
        legacyWrite = { legacyCalls += 1 },
      )
    }
    assertEquals(0, legacyCalls)
  }

  @Test
  fun invokesSelectedNativeWriterOnceWithExactInputs() {
    val descriptors = intArrayOf(11, 12, 13)
    val names = arrayOf("00000001.jpg", "00000002.webp", "ComicInfo.xml")
    var calls = 0

    val writtenBytes =
      writeGeneratedCbzDescriptorV1(descriptors, names, 17, 2) {
        receivedDescriptors,
        receivedNames,
        receivedOutput,
        receivedImages ->
        calls += 1
        assertContentEquals(descriptors, receivedDescriptors)
        assertContentEquals(names, receivedNames)
        assertEquals(17, receivedOutput)
        assertEquals(2, receivedImages)
        4_096L
      }

    assertEquals(4_096L, writtenBytes)
    assertEquals(1, calls)
  }

  @Test
  fun rejectsInvalidInputsBeforeCallingNativeCode() {
    var calls = 0
    val nativeCall = { _: IntArray, _: Array<String>, _: Int, _: Int ->
      calls += 1
      1L
    }

    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(
        intArrayOf(1, 2),
        arrayOf("page", "metadata"),
        -1,
        1,
        nativeCall,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(intArrayOf(1), arrayOf("page"), 3, 0, nativeCall)
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(intArrayOf(1), arrayOf("page"), 3, Int.MAX_VALUE, nativeCall)
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(intArrayOf(1), arrayOf("page"), 3, 100_000_000, nativeCall)
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(intArrayOf(1), arrayOf("page"), 3, 1, nativeCall)
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(
        intArrayOf(1, 2),
        arrayOf("page"),
        3,
        1,
        nativeCall,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      writeGeneratedCbzDescriptorV1(
        intArrayOf(1, -1),
        arrayOf("page", "metadata"),
        3,
        1,
        nativeCall,
      )
    }
    assertEquals(0, calls)
  }

  @Test
  fun rejectsInvalidNativeWriteSummaries() {
    val descriptors = intArrayOf(1, 2)
    val names = arrayOf("00000001.jpg", "ComicInfo.xml")
    assertFailsWith<IllegalStateException> {
      writeGeneratedCbzDescriptorV1(descriptors, names, 3, 1) { _, _, _, _ -> 0L }
    }
    assertFailsWith<IllegalStateException> {
      writeGeneratedCbzDescriptorV1(descriptors, names, 3, 1) { _, _, _, _ -> -1L }
    }
  }
}
