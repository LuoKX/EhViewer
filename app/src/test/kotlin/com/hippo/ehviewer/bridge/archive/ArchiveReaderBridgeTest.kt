package com.hippo.ehviewer.bridge.archive

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class ArchiveReaderBridgeTest {
  @Test
  fun opensExtractsAndClosesOneOpaqueReader() {
    val nativeApi = FakeArchiveReaderNativeApi()
    val reader = RustArchiveReaderV2.open(23, sortEntries = true, nativeApi)

    assertEquals(2, reader.entryCount)
    assertEquals("jpg", reader.extension(0))
    val output = reader.extract(0)
    assertTrue(output.isDirect)
    val actual = ByteArray(output.remaining())
    output.get(actual)
    assertContentEquals(FakeArchiveReaderNativeApi.IMAGE, actual)
    assertTrue(reader.extractToFileDescriptor(1, 29))

    reader.close()
    reader.close()
    assertEquals(1, nativeApi.openCalls)
    assertEquals(1, nativeApi.closeCalls)
    assertEquals(23, nativeApi.openedFileDescriptor)
    assertTrue(nativeApi.sortedEntries)
    assertEquals(29, nativeApi.outputFileDescriptor)
  }

  @Test
  fun rejectsInvalidInputsAndNativeSummaries() {
    assertFailsWith<IllegalArgumentException> {
      RustArchiveReaderV2.open(-1, false, FakeArchiveReaderNativeApi())
    }

    val zeroCount = FakeArchiveReaderNativeApi(entryCount = 0)
    assertFailsWith<IllegalStateException> {
      RustArchiveReaderV2.open(1, false, zeroCount)
    }
    assertEquals(1, zeroCount.closeCalls)

    val zeroHandle = FakeArchiveReaderNativeApi(readerHandle = 0)
    assertFailsWith<IllegalStateException> {
      RustArchiveReaderV2.open(1, false, zeroHandle)
    }

    val invalidSize =
      RustArchiveReaderV2.open(
        1,
        false,
        FakeArchiveReaderNativeApi(entrySize = 0),
      )
    assertFailsWith<IllegalStateException> { invalidSize.extract(0) }

    val mismatched =
      RustArchiveReaderV2.open(
        1,
        false,
        FakeArchiveReaderNativeApi(extractedSize = 1),
      )
    assertFailsWith<IllegalStateException> { mismatched.extract(0) }
  }

  @Test
  fun validatesIndexDescriptorExtensionAndClosedStateBeforeNativeCalls() {
    val emptyExtension =
      RustArchiveReaderV2.open(
        1,
        false,
        FakeArchiveReaderNativeApi(extension = ""),
      )
    assertFailsWith<IllegalStateException> { emptyExtension.extension(0) }

    val reader = RustArchiveReaderV2.open(1, false, FakeArchiveReaderNativeApi())
    assertFailsWith<IllegalArgumentException> { reader.extension(-1) }
    assertFailsWith<IllegalArgumentException> { reader.extract(reader.entryCount) }
    assertFailsWith<IllegalArgumentException> { reader.extractToFileDescriptor(0, -1) }
    reader.close()
    assertFailsWith<IllegalStateException> { reader.extension(0) }
  }

  @Test
  fun passwordStateAndRetryStayOnTheRustHandle() {
    val nativeApi =
      FakeArchiveReaderNativeApi(
        needsPassword = true,
        acceptedPassword = "secret".encodeToByteArray(),
      )
    val reader = RustArchiveReaderV2.open(1, false, nativeApi)

    assertTrue(reader.needsPassword)
    assertFalse(reader.providePassword("wrong"))
    assertContentEquals(ByteArray("wrong".length), nativeApi.lastPassword)
    assertTrue(reader.providePassword("secret"))
    assertContentEquals(ByteArray("secret".length), nativeApi.lastPassword)
    assertFalse(reader.needsPassword)
    assertEquals(2, reader.entryCount)
  }

  private class FakeArchiveReaderNativeApi(
    private val readerHandle: Long = 7,
    private val entryCount: Int = 2,
    private val entrySize: Long = IMAGE.size.toLong(),
    private val extension: String = "jpg",
    private val extractedSize: Int = IMAGE.size,
    private var needsPassword: Boolean = false,
    private val acceptedPassword: ByteArray? = null,
  ) : ArchiveReaderNativeApi {
    var openCalls = 0
    var closeCalls = 0
    var openedFileDescriptor = -1
    var outputFileDescriptor = -1
    var sortedEntries = false
    var lastPassword = byteArrayOf()

    override fun open(
      fileDescriptor: Int,
      sortEntries: Boolean,
    ): Long {
      openCalls += 1
      openedFileDescriptor = fileDescriptor
      sortedEntries = sortEntries
      return readerHandle
    }

    override fun needsPassword(readerHandle: Long) = needsPassword

    override fun providePassword(
      readerHandle: Long,
      password: ByteArray,
    ): Boolean {
      lastPassword = password
      val accepted = acceptedPassword?.contentEquals(password) == true
      if (accepted) needsPassword = false
      return accepted
    }

    override fun entryCount(readerHandle: Long) = entryCount

    override fun entrySize(
      readerHandle: Long,
      index: Int,
    ) = entrySize

    override fun extension(
      readerHandle: Long,
      index: Int,
    ) = extension

    override fun name(
      readerHandle: Long,
      index: Int,
    ) = "0000000${index + 1}.$extension"

    override fun verify(
      readerHandle: Long,
      index: Int,
    ) = true

    override fun hasUnsafePath(readerHandle: Long) = false

    override fun recoveryMetadata(
      readerHandle: Long,
      kind: Int,
    ): ByteArray? = null

    override fun inspectRecovery(
      readerHandle: Long,
      request: ByteBuffer,
      requestSize: Int,
    ): ByteArray = error("V2 characterization never invokes recovery batches")

    override fun extract(
      readerHandle: Long,
      index: Int,
      output: ByteBuffer,
    ): Int {
      output.duplicate().put(IMAGE)
      return extractedSize
    }

    override fun extractToFileDescriptor(
      readerHandle: Long,
      index: Int,
      fileDescriptor: Int,
    ): Boolean {
      outputFileDescriptor = fileDescriptor
      return true
    }

    override fun close(readerHandle: Long): Boolean {
      closeCalls += 1
      return true
    }

    companion object {
      val IMAGE = byteArrayOf(1, 2, 3, 4)
    }
  }
}
