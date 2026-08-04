package com.hippo.ehviewer.bridge.archive

import android.os.ParcelFileDescriptor
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.io.File
import java.util.Collections
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import okio.Path.Companion.toOkioPath
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Device contract tests for generated-CBZ descriptor ownership and JNI round trips. */
@RunWith(AndroidJUnit4::class)
class ArchiveWriterDeviceTest {
  @Test
  fun writesAndValidatesRealDescriptorBackedCbz() {
    val pageOne = temporaryFile("page-one", ".jpg").apply { writeBytes(PAGE_ONE) }
    val pageTwo = temporaryFile("page-two", ".webp").apply { writeBytes(PAGE_TWO) }
    val comicInfo = temporaryFile("comic-info", ".xml").apply { writeBytes(COMIC_INFO) }
    val output = temporaryFile("generated", ".cbz")

    try {
      val writtenBytes =
        openReadOnly(pageOne).use { pageOneDescriptor ->
          openReadOnly(pageTwo).use { pageTwoDescriptor ->
            openReadOnly(comicInfo).use { metadataDescriptor ->
              openTruncated(output).use { outputDescriptor ->
                writeGeneratedCbzDescriptorV1(
                  sourceDescriptors =
                    intArrayOf(pageOneDescriptor.fd, pageTwoDescriptor.fd, metadataDescriptor.fd),
                  entryNames = arrayOf("00000001.jpg", "00000002.webp", "ComicInfo.xml"),
                  outputDescriptor = outputDescriptor.fd,
                  expectedImages = 2,
                )
              }
            }
          }
        }

      val expectedBytes = (PAGE_ONE.size + PAGE_TWO.size + COMIC_INFO.size).toLong()
      assertEquals(expectedBytes, writtenBytes)
      assertEquals(expectedBytes, validateDownloadedCbzV1(output.toOkioPath(), 2))

      ZipFile(output).use { archive ->
        val entries = Collections.list(archive.entries())
        assertEquals(
          listOf("00000001.jpg", "00000002.webp", "ComicInfo.xml"),
          entries.map(ZipEntry::getName),
        )
        entries.forEach { entry -> assertEquals(ZipEntry.STORED, entry.method) }
        assertArrayEquals(PAGE_ONE, archive.getInputStream(entries[0]).use { it.readBytes() })
        assertArrayEquals(PAGE_TWO, archive.getInputStream(entries[1]).use { it.readBytes() })
        assertArrayEquals(COMIC_INFO, archive.getInputStream(entries[2]).use { it.readBytes() })
      }
    } finally {
      pageOne.delete()
      pageTwo.delete()
      comicInfo.delete()
      output.delete()
    }
  }

  @Test
  fun failedWriteTruncatesPartialOutput() {
    val emptyPage = temporaryFile("empty-page", ".jpg")
    val comicInfo = temporaryFile("failure-metadata", ".xml").apply { writeBytes(COMIC_INFO) }
    val output = temporaryFile("failed-generated", ".cbz").apply { writeText("stale") }

    try {
      openReadOnly(emptyPage).use { pageDescriptor ->
        openReadOnly(comicInfo).use { metadataDescriptor ->
          openTruncated(output).use { outputDescriptor ->
            assertThrows(RuntimeException::class.java) {
              writeGeneratedCbzDescriptorV1(
                sourceDescriptors = intArrayOf(pageDescriptor.fd, metadataDescriptor.fd),
                entryNames = arrayOf("00000001.jpg", "ComicInfo.xml"),
                outputDescriptor = outputDescriptor.fd,
                expectedImages = 1,
              )
            }
          }
        }
      }
      assertEquals(0L, output.length())
    } finally {
      emptyPage.delete()
      comicInfo.delete()
      output.delete()
    }
  }

  private fun temporaryFile(prefix: String, suffix: String): File =
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
    val PAGE_ONE = "page-one-unique-payload".encodeToByteArray()
    val PAGE_TWO = "page-two-unique-payload".encodeToByteArray()
    val COMIC_INFO = "<ComicInfo><PageCount>2</PageCount></ComicInfo>".encodeToByteArray()
  }
}
