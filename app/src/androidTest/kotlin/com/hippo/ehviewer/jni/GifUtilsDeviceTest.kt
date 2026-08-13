package com.hippo.ehviewer.jni

import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.nio.ByteBuffer
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Offline device contracts for Rust-owned GIF preparation and direct-buffer rewriting. */
@RunWith(AndroidJUnit4::class)
class GifUtilsDeviceTest {
  @Test
  fun preparesRepeatedOwnedCopiesWithoutMovingOrMutatingTheSource() =
    withFixture(fastGif()) { file, original ->
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
        Os.lseek(descriptor.fileDescriptor, 3, OsConstants.SEEK_SET)
        repeat(3) {
          val prepared = requireNotNull(prepareGifSourceOwned(descriptor.fd))
          assertEquals(original.size, prepared.size)
          assertArrayEquals(byteArrayOf(10, 0), prepared.copyOfRange(11, 13))
          assertEquals(3L, Os.lseek(descriptor.fileDescriptor, 0, OsConstants.SEEK_CUR))
        }
      }
      assertArrayEquals(original, file.readBytes())
    }

  @Test
  fun nonGifReadsOnlyAsAClassificationAndLeavesTheDescriptorPosition() =
    withFixture(ByteArray(1024 * 1024) { 0x55 }) { file, original ->
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
        Os.lseek(descriptor.fileDescriptor, 17, OsConstants.SEEK_SET)
        assertNull(prepareGifSourceOwned(descriptor.fd))
        assertEquals(17L, Os.lseek(descriptor.fileDescriptor, 0, OsConstants.SEEK_CUR))
      }
      assertArrayEquals(original, file.readBytes())
    }

  @Test
  fun rewritesDirectBuffersAndContainsHeapBufferFailures() {
    val original = fastGif()
    val direct = ByteBuffer.allocateDirect(original.size).apply { put(original) }
    rewriteGifSource(direct)
    assertEquals(10, direct.get(11).toInt())
    assertEquals(0, direct.get(12).toInt())

    assertThrows(RuntimeException::class.java) {
      rewriteGifSource(ByteBuffer.wrap(original))
    }

    val truncated = ByteBuffer.allocateDirect(7).apply { put("GIF89a\u0000".encodeToByteArray()) }
    rewriteGifSource(truncated)
    assertArrayEquals("GIF89a\u0000".encodeToByteArray(), truncated.toByteArray())

    assertThrows(RuntimeException::class.java) { prepareGifSourceOwned(-1) }
  }

  @Test
  fun preparesARepresentativeLargeFixtureWithoutChangingItsBytes() {
    val original = ByteArray(8 * 1024 * 1024) { 0x55 }
    "GIF89a".encodeToByteArray().copyInto(original)
    fastControlExtension().copyInto(original, original.size - fastControlExtension().size)

    withFixture(original) { file, source ->
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
        val prepared = requireNotNull(prepareGifSourceOwned(descriptor.fd))
        assertEquals(source.size, prepared.size)
        assertArrayEquals(
          byteArrayOf(10, 0),
          prepared.copyOfRange(prepared.size - 4, prepared.size - 2),
        )
      }
      assertArrayEquals(source, file.readBytes())
    }
  }

  private fun fastGif() =
    "GIF89a".encodeToByteArray() + fastControlExtension() + "tail".encodeToByteArray()

  private fun fastControlExtension() = byteArrayOf(0, 0x21, 0xf9.toByte(), 4, 0, 1, 0, 0, 0)

  private inline fun withFixture(source: ByteArray, block: (File, ByteArray) -> Unit) {
    val file =
      File.createTempFile(
        "ehviewer-gif-",
        ".fixture",
        InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,
      )
    try {
      file.writeBytes(source)
      block(file, source.copyOf())
    } finally {
      file.delete()
    }
  }

  private fun ByteBuffer.toByteArray() =
    ByteArray(capacity()).also { duplicate().apply { clear() }.get(it) }
}
