package com.hippo.ehviewer.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class SettingsRuleBridgeTest {
  @Test
  fun legacySettingsRepairPreservesExistingRules() {
    val plan =
      resolveSettingsRepair(
        SettingsRepairRequest.create(
          chineseLocale = true,
          showTagTranslationsAbsent = true,
          downloadFilterMode = 1,
          recentDownloadLabel = null,
          orientationMode = 99,
          validOrientationModes = listOf(0, 1, 2),
        ),
        CoreBackend.LEGACY,
      )

    assertTrue(plan.enableTagTranslations)
    assertTrue(plan.resetRecentDownloadLabel)
    assertTrue(plan.removeOrientationMode)
  }

  @Test
  fun legacyReadCacheSizeUsesExistingBounds() {
    assertEquals(
      320,
      resolveReadCacheSize(ReadCacheSizeRequest.create(1), CoreBackend.LEGACY),
    )
    assertEquals(
      5120,
      resolveReadCacheSize(ReadCacheSizeRequest.create(9000), CoreBackend.LEGACY),
    )
  }

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
          ),
          CoreBackend.RUST,
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
      resolveReadCacheSize(ReadCacheSizeRequest.create(1), CoreBackend.RUST) {
        invoked = true
        ReadCacheSizePlan(777)
      }

    assertTrue(invoked)
    assertFalse(size == 320)
    assertEquals(777, size)
  }
}
