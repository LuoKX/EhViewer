package com.hippo.ehviewer.bridge

import org.junit.Assert.assertEquals
import org.junit.Test

class CoreBackendSelectorTest {
  @Test
  fun mapsTheGlobalSettingToOneExclusiveBackend() {
    assertEquals(CoreBackend.LEGACY, selectCoreBackend(useRustCore = false))
    assertEquals(CoreBackend.RUST, selectCoreBackend(useRustCore = true))
  }
}
