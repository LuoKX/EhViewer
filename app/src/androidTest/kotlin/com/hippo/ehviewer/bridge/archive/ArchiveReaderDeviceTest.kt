package com.hippo.ehviewer.bridge.archive

import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.nio.ByteBuffer
import java.util.zip.CRC32
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Device contract tests for the Rust archive reader's JNI and descriptor ownership boundary. */
@RunWith(AndroidJUnit4::class)
class ArchiveReaderDeviceTest {
  @Test
  fun opensExtractsAndClosesRealDescriptorBackedArchive() {
    val archive = temporaryFile("reader-valid", ".cbz")
    val extracted = temporaryFile("reader-extracted", ".jpg")
    writeArchive(archive)

    try {
      openReadOnly(archive).use { sourceDescriptor ->
        Os.lseek(sourceDescriptor.fileDescriptor, SOURCE_OFFSET, OsConstants.SEEK_SET)
        val reader =
          RustArchiveReaderV2.open(
            fileDescriptor = sourceDescriptor.fd,
            sortEntries = true,
          )
        try {
          assertSourceOffset(sourceDescriptor)
          assertEquals(2, reader.entryCount)
          assertEquals("png", reader.extension(0))
          assertEquals("jpg", reader.extension(1))

          val stored = reader.extract(0)
          assertTrue(stored.isDirect)
          assertArrayEquals(PAGE_TWO, stored.readRemainingBytes())
          assertSourceOffset(sourceDescriptor)

          val deflated = reader.extract(1)
          assertTrue(deflated.isDirect)
          assertArrayEquals(PAGE_TEN, deflated.readRemainingBytes())
          assertSourceOffset(sourceDescriptor)

          openTruncated(extracted).use { outputDescriptor ->
            assertTrue(reader.extractToFileDescriptor(1, outputDescriptor.fd))
          }
          assertArrayEquals(PAGE_TEN, extracted.readBytes())
          assertSourceOffset(sourceDescriptor)
        } finally {
          reader.close()
          reader.close()
        }

        assertSourceOffset(sourceDescriptor)
        assertThrows(IllegalStateException::class.java) { reader.extension(0) }
      }
    } finally {
      archive.delete()
      extracted.delete()
    }
  }

  @Test
  fun containsCrcFailuresAndRestoresTheSourceDescriptor() {
    val archive = temporaryFile("reader-corrupt", ".cbz")
    val extracted = temporaryFile("reader-corrupt-output", ".jpg")
    writeStoredArchive(archive, "page.jpg", PAGE_TWO)
    corruptFirstCentralDirectoryCrc(archive)

    try {
      openReadOnly(archive).use { sourceDescriptor ->
        Os.lseek(sourceDescriptor.fileDescriptor, SOURCE_OFFSET, OsConstants.SEEK_SET)
        val reader = RustArchiveReaderV2.open(sourceDescriptor.fd, sortEntries = false)
        try {
          assertThrows(RuntimeException::class.java) { reader.extract(0) }
          assertSourceOffset(sourceDescriptor)

          openTruncated(extracted).use { outputDescriptor ->
            assertThrows(RuntimeException::class.java) {
              reader.extractToFileDescriptor(0, outputDescriptor.fd)
            }
          }
          assertSourceOffset(sourceDescriptor)
        } finally {
          reader.close()
          reader.close()
        }
      }
    } finally {
      archive.delete()
      extracted.delete()
    }
  }

  @Test
  fun detectsAndExtractsSupportedFormatsWithoutTrustingTheOuterExtension() {
    val fixtures =
      listOf(
        ArchiveFixture("archive-v2/rar4-images.rar", 2, "png", RAR4_PAGE),
        ArchiveFixture("archive-v2/rar5-images.rar", 1, "png", RAR5_PAGE),
        ArchiveFixture("archive-v2/images.7z", 1, "png", GENERATED_PAGE),
        ArchiveFixture("archive-v2/images.tar", 1, "png", GENERATED_PAGE),
      )

    fixtures.forEach { fixture ->
      val archive = copyAssetToTemporaryFile(fixture.assetName, ".misleading")
      try {
        openReadOnly(archive).use { sourceDescriptor ->
          RustArchiveReaderV2.open(sourceDescriptor.fd, sortEntries = false).use { reader ->
            assertEquals(fixture.entryCount, reader.entryCount)
            assertEquals(fixture.extension, reader.extension(0))
            assertArrayEquals(fixture.payload, reader.extract(0).readRemainingBytes())
          }
        }
      } finally {
        archive.delete()
      }
    }
  }

  @Test
  fun rejectsWrongPasswordsAndExtractsEncryptedFormatMatrix() {
    val fixtures =
      listOf(
        EncryptedArchiveFixture("archive-v2/encrypted-images.zip", "secret", GENERATED_PAGE),
        EncryptedArchiveFixture("archive-v2/encrypted-images.7z", "secret", GENERATED_PAGE),
        EncryptedArchiveFixture("archive-v2/rar4-encrypted-images.rar", "password", RAR_PAGE),
        EncryptedArchiveFixture("archive-v2/rar5-encrypted-images.rar", "password", RAR_PAGE),
      )

    fixtures.forEach { fixture ->
      val archive = copyAssetToTemporaryFile(fixture.assetName, ".locked")
      try {
        openReadOnly(archive).use { sourceDescriptor ->
          RustArchiveReaderV2.open(sourceDescriptor.fd, sortEntries = false).use { reader ->
            assertTrue(reader.needsPassword)
            assertTrue(!reader.providePassword("wrong"))
            assertTrue(reader.providePassword(fixture.password))
            assertEquals("png", reader.extension(0))
            assertArrayEquals(fixture.payload, reader.extract(0).readRemainingBytes())
          }
        }
      } finally {
        archive.delete()
      }
    }
  }

  @Test
  fun recoveryV3BatchesPreserveBorrowedDescriptorsAndDetectCrcFailure() {
    val archive = temporaryFile("recovery-v3", ".cbz")
    ZipOutputStream(archive.outputStream().buffered()).use { zip ->
      zip.writeEntry("1.jpg", PAGE_TWO, ZipEntry.STORED)
      zip.writeEntry("2.jpg", PAGE_TEN, ZipEntry.DEFLATED)
    }
    try {
      openReadOnly(archive).use { descriptor ->
        Os.lseek(descriptor.fileDescriptor, SOURCE_OFFSET, OsConstants.SEEK_SET)
        RustArchiveReaderV2.open(descriptor.fd, true).use { reader ->
          val first = reader.inspectRecovery(ArchiveRecoveryRequest(3, 0, 1, 100_000, true))
          assertEquals(1, first.nextIndex)
          assertTrue(!first.complete)
          assertSourceOffset(descriptor)
          val last =
            reader.inspectRecovery(ArchiveRecoveryRequest(3, 1, 1, 100_000, first.allReadable))
          assertTrue(last.complete)
          assertSourceOffset(descriptor)
          assertThrows(RuntimeException::class.java) {
            reader.inspectRecovery(ArchiveRecoveryRequest(3, 1, 1, 100_000, true))
          }
        }
        assertSourceOffset(descriptor)
      }
      corruptFirstCentralDirectoryCrc(archive)
      openReadOnly(archive).use { descriptor ->
        RustArchiveReaderV2.open(descriptor.fd, true).use { reader ->
          val first = reader.inspectRecovery(ArchiveRecoveryRequest(3, 0, 1, 100_000, true))
          assertTrue(!first.allReadable)
          val last =
            reader.inspectRecovery(ArchiveRecoveryRequest(3, 1, 1, 100_000, first.allReadable))
          assertTrue(!last.complete)
        }
      }
    } finally {
      archive.delete()
    }
  }

  @Test
  fun recoveryV3ReturnsStructuredLimitsAndEncryptionWithoutFallback() {
    val archive = temporaryFile("recovery-v3-limit", ".cbz")
    ZipOutputStream(archive.outputStream().buffered()).use { zip ->
      zip.writeEntry("1.jpg", PAGE_TWO, ZipEntry.STORED)
      zip.writeEntry("2.jpg", PAGE_TEN, ZipEntry.STORED)
    }
    try {
      openReadOnly(archive).use { descriptor ->
        RustArchiveReaderV2.open(descriptor.fd, true).use { reader ->
          val batch = reader.inspectRecovery(ArchiveRecoveryRequest(3, 0, 32, 1, true))
          assertTrue(batch.entries.isEmpty())
          assertEquals(
            listOf(com.hippo.ehviewer.bridge.download.RecoveryIssue.LIMIT_EXCEEDED),
            batch.issues,
          )
        }
      }
    } finally {
      archive.delete()
    }
    val locked = copyAssetToTemporaryFile("archive-v2/encrypted-images.zip", ".locked")
    try {
      openReadOnly(locked).use { descriptor ->
        RustArchiveReaderV2.open(descriptor.fd, false).use { reader ->
          val batch = reader.inspectRecovery(ArchiveRecoveryRequest(3, 0, 32, 100_000, true))
          assertEquals(
            listOf(com.hippo.ehviewer.bridge.download.RecoveryIssue.ENCRYPTED_ARCHIVE),
            batch.issues,
          )
          assertTrue(batch.entries.isEmpty() && !batch.complete)
        }
      }
    } finally {
      locked.delete()
    }
  }

  private fun writeArchive(file: File) {
    ZipOutputStream(file.outputStream().buffered()).use { archive ->
      archive.writeEntry("folder/image10.jpg", PAGE_TEN, ZipEntry.DEFLATED)
      archive.writeEntry("ComicInfo.xml", COMIC_INFO, ZipEntry.DEFLATED)
      archive.writeEntry("folder/image2.png", PAGE_TWO, ZipEntry.STORED)
      archive.writeEntry("folder/ignored.JPG", UPPERCASE_IMAGE, ZipEntry.STORED)
    }
  }

  private fun writeStoredArchive(
    file: File,
    name: String,
    contents: ByteArray,
  ) {
    ZipOutputStream(file.outputStream().buffered()).use { archive ->
      archive.writeEntry(name, contents, ZipEntry.STORED)
    }
  }

  private fun ZipOutputStream.writeEntry(
    name: String,
    contents: ByteArray,
    method: Int,
  ) {
    val entry = ZipEntry(name).apply { this.method = method }
    if (method == ZipEntry.STORED) {
      val size = contents.size.toLong()
      entry.size = size
      entry.compressedSize = size
      entry.crc = CRC32().apply { update(contents) }.value
    }
    putNextEntry(entry)
    write(contents)
    closeEntry()
  }

  private fun corruptFirstCentralDirectoryCrc(file: File) {
    val bytes = file.readBytes()
    val centralDirectory = signatureOffset(bytes, CENTRAL_DIRECTORY_SIGNATURE)
    val crcOffset = centralDirectory + CENTRAL_DIRECTORY_CRC_OFFSET
    bytes[crcOffset] = (bytes[crcOffset].toInt() xor 0x80).toByte()
    file.writeBytes(bytes)
  }

  private fun signatureOffset(
    bytes: ByteArray,
    signature: ByteArray,
  ): Int {
    require(bytes.size >= signature.size) { "ZIP fixture is shorter than its signature" }
    for (index in 0..(bytes.size - signature.size)) {
      if (signature.indices.all { offset -> bytes[index + offset] == signature[offset] }) {
        return index
      }
    }
    error("ZIP fixture signature is missing")
  }

  private fun ByteBuffer.readRemainingBytes(): ByteArray =
    ByteArray(remaining()).also { bytes -> get(bytes) }

  private fun copyAssetToTemporaryFile(
    assetName: String,
    suffix: String,
  ): File =
    temporaryFile("reader-fixture", suffix).also { output ->
      ApplicationProvider.getApplicationContext<android.content.Context>()
        .assets
        .open(assetName)
        .use { input -> output.outputStream().use(input::copyTo) }
    }

  private fun assertSourceOffset(descriptor: ParcelFileDescriptor) {
    assertEquals(
      "Rust must restore the Android-owned source descriptor offset",
      SOURCE_OFFSET,
      Os.lseek(descriptor.fileDescriptor, 0L, OsConstants.SEEK_CUR),
    )
  }

  private fun temporaryFile(
    prefix: String,
    suffix: String,
  ): File =
    File.createTempFile(
      prefix,
      suffix,
      ApplicationProvider.getApplicationContext<android.content.Context>().cacheDir,
    )

  private fun openReadOnly(file: File) =
    ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)

  private fun openTruncated(file: File) =
    ParcelFileDescriptor.open(
      file,
      ParcelFileDescriptor.MODE_CREATE or
        ParcelFileDescriptor.MODE_READ_WRITE or
        ParcelFileDescriptor.MODE_TRUNCATE,
    )

  private companion object {
    data class ArchiveFixture(
      val assetName: String,
      val entryCount: Int,
      val extension: String,
      val payload: ByteArray,
    )

    data class EncryptedArchiveFixture(
      val assetName: String,
      val password: String,
      val payload: ByteArray,
    )

    const val SOURCE_OFFSET = 3L
    const val CENTRAL_DIRECTORY_CRC_OFFSET = 16
    val CENTRAL_DIRECTORY_SIGNATURE = byteArrayOf(0x50, 0x4b, 0x01, 0x02)
    val PAGE_TWO = "second-image-payload".encodeToByteArray()
    val PAGE_TEN = "tenth-image-payload".encodeToByteArray()
    val COMIC_INFO = "<ComicInfo><PageCount>2</PageCount></ComicInfo>".encodeToByteArray()
    val UPPERCASE_IMAGE = "ignored-uppercase-extension".encodeToByteArray()
    val GENERATED_PAGE = "archive-v2-page\n".encodeToByteArray()
    val RAR4_PAGE = "test text document\r\n".encodeToByteArray()
    val RAR5_PAGE = "hello libarchive test suite!\n".encodeToByteArray()
    val RAR_PAGE = "This is from a.txt".encodeToByteArray()
  }
}
