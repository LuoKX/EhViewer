package com.hippo.ehviewer.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class SettingsRuleBridgeTest {
  @Test
  fun rustSettingsRepairFailureDoesNotFallBack() {
    val expected = IllegalStateException("native failure")
    var invoked = false

    val actual =
      try {
        resolveSettingsRepair(
          SettingsRepairRequest.create(
            chineseLocale = false,
            showTagTranslationsAbsent = false,
            downloadFilterMode = 0,
            recentDownloadLabel = "label",
            orientationMode = 1,
            validOrientationModes = listOf(0, 1, 2),
          )
        ) {
          invoked = true
          throw expected
        }
        null
      } catch (error: IllegalStateException) {
        error
      }

    assertTrue(invoked)
    assertSame(expected, actual)
  }

  @Test
  fun rustReadCacheSizeReturnsRustPlanDirectly() {
    var invoked = false
    val size =
      resolveReadCacheSize(ReadCacheSizeRequest.create(1)) {
        invoked = true
        ReadCacheSizePlan(777)
      }

    assertTrue(invoked)
    assertFalse(size == 320)
    assertEquals(777, size)
  }
}
