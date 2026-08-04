package com.hippo.ehviewer.bridge.archive

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class ArchiveValidationBridgeTest {
  @Test
  fun invokesSelectedNativeValidatorOnceWithExactInputs() {
    var calls = 0
    var receivedDescriptor = -1
    var receivedImages = -1

    val validatedBytes =
      validateDownloadedCbzDescriptorV1(41, 7) { fileDescriptor, expectedImages ->
        calls += 1
        receivedDescriptor = fileDescriptor
        receivedImages = expectedImages
        4_096L
      }

    assertEquals(4_096L, validatedBytes)
    assertEquals(1, calls)
    assertEquals(41, receivedDescriptor)
    assertEquals(7, receivedImages)
  }

  @Test
  fun rejectsInvalidInputsBeforeCallingNativeCode() {
    var calls = 0
    val nativeCall = { _: Int, _: Int ->
      calls += 1
      1L
    }

    assertFailsWith<IllegalArgumentException> {
      validateDownloadedCbzDescriptorV1(-1, 1, nativeCall)
    }
    assertFailsWith<IllegalArgumentException> {
      validateDownloadedCbzDescriptorV1(1, 0, nativeCall)
    }
    assertEquals(0, calls)
  }

  @Test
  fun rejectsInvalidNativeValidationSummaries() {
    assertFailsWith<IllegalStateException> {
      validateDownloadedCbzDescriptorV1(1, 1) { _, _ -> 0L }
    }
    assertFailsWith<IllegalStateException> {
      validateDownloadedCbzDescriptorV1(1, 1) { _, _ -> -1L }
    }
  }
}
