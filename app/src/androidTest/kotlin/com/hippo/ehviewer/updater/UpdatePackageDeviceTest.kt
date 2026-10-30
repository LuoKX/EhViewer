package com.hippo.ehviewer.updater

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.hippo.ehviewer.bridge.updater.extractUpdateApk
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class UpdatePackageDeviceTest {
  @Test
  fun realDescriptorsSelectOnlyApkAndRejectAmbiguousArchives() {
    val context = InstrumentationRegistry.getInstrumentation().targetContext
    val input = File.createTempFile("update-source", ".zip", context.cacheDir)
    val output = File.createTempFile("update-target", ".apk", context.cacheDir)
    try {
      fun zip(vararg names: String) {
        ZipOutputStream(input.outputStream()).use { archive ->
          names.forEach { name ->
            archive.putNextEntry(ZipEntry(name))
            if (!name.endsWith('/')) archive.write(name.encodeToByteArray())
            archive.closeEntry()
          }
        }
      }
      zip("docs/", "README", "nested/update.apk")
      extractUpdateApk(input, output)
      assertEquals("nested/update.apk", output.readText())
      for (names in
        listOf(
          emptyArray(),
          arrayOf("README"),
          arrayOf("one.apk", "two.apk"),
          arrayOf("../bad.apk"),
        )) {
        zip(*names)
        try {
          extractUpdateApk(input, output)
          fail("Invalid update accepted")
        } catch (_: RuntimeException) {}
        assertEquals(0L, output.length())
      }
    } finally {
      input.delete()
      output.delete()
    }
  }
}
