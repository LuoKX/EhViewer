package com.hippo.ehviewer

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RustCorePreferenceMigrationTest {
  @Test
  fun newKeyWinsOverLegacyValues() {
    assertTrue(resolveMigratedRustCoreEnabled(true, List(32) { false }))
    assertFalse(resolveMigratedRustCoreEnabled(false, List(32) { true }))
  }

  @Test
  fun allLegacyKeysMustBePresentAndEnabled() {
    assertTrue(resolveMigratedRustCoreEnabled(null, List(32) { true }))
    assertFalse(resolveMigratedRustCoreEnabled(null, List(31) { true } + null))
    assertFalse(resolveMigratedRustCoreEnabled(null, List(31) { true } + false))
    assertFalse(resolveMigratedRustCoreEnabled(null, List(32) { null }))
  }

  @Test
  fun repeatedMigrationPreservesTheUnifiedValue() {
    val first = resolveMigratedRustCoreEnabled(null, List(32) { true })
    assertTrue(resolveMigratedRustCoreEnabled(first, List(32) { null }))
  }
}
