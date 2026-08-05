package com.hippo.ehviewer.download.recovery

import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SafTreeWalkerTest {
  @Test
  fun compatibilityCheckDistinguishesPlainFilesAndSymbolicLinks() {
    val directory = Files.createTempDirectory("recovery-symlink-test").toFile()
    try {
      val target = File(directory, "target").apply { writeText("data") }
      val link = File(directory, "link")
      Files.createSymbolicLink(link.toPath(), target.toPath())

      assertFalse(target.isSymbolicLinkCompat())
      assertTrue(link.isSymbolicLinkCompat())
    } finally {
      directory.deleteRecursively()
    }
  }
}
