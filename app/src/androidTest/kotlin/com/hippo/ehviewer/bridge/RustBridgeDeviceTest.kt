package com.hippo.ehviewer.bridge

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Offline API-35 real-JNI health contract; local runtime execution is explicitly UNRUN. */
@RunWith(AndroidJUnit4::class)
class RustBridgeDeviceTest {
  @Test
  fun rustCoreHealthContractIsReachableWithoutHttp() {
    assertTrue(RustBridge.isReady())
    assertTrue(RustBridge.checkVersion())
    assertFalse(RustBridge.checkVersion(expectedVersion = 2))
    assertTrue(RustBridge.ping())
    assertTrue(RustBridge.ping(Long.MIN_VALUE))
    assertTrue(RustBridge.ping(Long.MAX_VALUE))
  }
}
