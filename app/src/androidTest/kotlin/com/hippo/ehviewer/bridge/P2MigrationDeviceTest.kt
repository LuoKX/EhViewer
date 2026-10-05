package com.hippo.ehviewer.bridge

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.gallery.*
import com.hippo.ehviewer.bridge.reader.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI probes; compilation is not device-execution evidence. */
@RunWith(AndroidJUnit4::class)
class P2MigrationDeviceTest {
  @Test
  fun allPortableOwnersUseRealNativeBoundaries() {
    assertEquals(
      64 * 1024 * 1024,
      planReaderPolicy(
          ReaderPolicyRequest(
            kind = ReaderPolicyKind.MEMORY,
            appMaxMemory = 268435456,
            totalMemory = 0,
          )
        )
        .cacheSize,
    )
    assertEquals(64, readerImageEntrySize(Long.MAX_VALUE, 64))
    val counters =
      planReaderPolicy(
        ReaderPolicyRequest(
          kind = ReaderPolicyKind.PAGE_TRANSITION,
          pages = 1,
          finished = 0,
          downloaded = 0,
          oldState = ReaderSpiderState.NONE,
          newState = ReaderSpiderState.FINISHED,
        )
      )
    assertEquals(ReaderPageCompletion.ARCHIVE, counters.completion)
    assertEquals(
      1L,
      planRecoverySearch(
          RecoverySearchRequest(cached = false, elapsedMillis = 2999, cacheSize = 128)
        )
        .waitMillis,
    )
    assertEquals(
      listOf(1, 0),
      selectLocalPages(LocalPagesRequest(names = listOf("00000002.jpg", "00000001.JPG", "ignored")))
        .map { it.sourceIndex },
    )
    assertEquals(
      "$" + "m/7-2",
      planCacheKey(CacheKeyRequest(kind = CacheKeyKind.PREVIEW, input = "/cm/t/7-2.webp")),
    )
    val info = SpiderInfoData(7, "0123456789", 3, mapOf(0 to "a", 2 to "b"), 40)
    val stored =
      codecSpiderInfo(SpiderInfoCodecRequest(kind = SpiderInfoCodecKind.ENCODE, info = info)).bytes
    assertEquals(
      info,
      codecSpiderInfo(
          SpiderInfoCodecRequest(
            kind = SpiderInfoCodecKind.DECODE,
            bytes = stored,
            allowLegacy = false,
          )
        )
        .info,
    )
    TransferSpeedHandle(1_000_000_000).use {
      assertNull(it.sample(0))
      it.track(0, 50)
      assertEquals(5000L, it.sample(10_000_000))
      it.reset()
      assertNull(it.sample(0))
    }
    assertThrows(RustFailureException::class.java) {
      planReaderPolicy(
        ReaderPolicyRequest(
          version = 2,
          kind = ReaderPolicyKind.MEMORY,
          appMaxMemory = 1,
          totalMemory = 0,
        )
      )
    }
  }
}
