package com.hippo.ehviewer.bridge.updater

import java.io.File

/**
 * Test-only real JNI bootstrap. Missing host artifact fails tests, never selects a fake backend.
 */
internal object UpdaterHostLibrary {
  val root: File =
    generateSequence(File(checkNotNull(System.getProperty("user.dir"))).absoluteFile) {
        it.parentFile
      }
      .first { File(it, "app/src/main/rust/Cargo.toml").isFile }
  private val loaded = run {
    System.load(File(root, "app/src/main/rust/target/jni-probe/libehviewer-host.so").absolutePath)
  }

  fun load() {
    loaded
  }

  fun golden(name: String): ByteArray =
    File(root, "app/src/main/rust/crates/net/test_data/updater/$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()
}
