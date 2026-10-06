package com.hippo.ehviewer.bridge.archive

import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

/** Injectable native calls behind the Rust multi-format archive reader V2 adapter. */
internal interface ArchiveReaderNativeApi {
  fun open(
    fileDescriptor: Int,
    sortEntries: Boolean,
  ): Long

  fun needsPassword(readerHandle: Long): Boolean

  fun providePassword(
    readerHandle: Long,
    password: ByteArray,
  ): Boolean

  fun entryCount(readerHandle: Long): Int

  fun entrySize(
    readerHandle: Long,
    index: Int,
  ): Long

  fun extension(
    readerHandle: Long,
    index: Int,
  ): String

  fun name(
    readerHandle: Long,
    index: Int,
  ): String

  fun verify(
    readerHandle: Long,
    index: Int,
  ): Boolean

  fun hasUnsafePath(readerHandle: Long): Boolean

  fun recoveryMetadata(
    readerHandle: Long,
    kind: Int,
  ): ByteArray?

  fun inspectRecovery(
    readerHandle: Long,
    request: ByteBuffer,
    requestSize: Int,
  ): ByteArray

  fun extract(
    readerHandle: Long,
    index: Int,
    output: ByteBuffer,
  ): Int

  fun extractToFileDescriptor(
    readerHandle: Long,
    index: Int,
    fileDescriptor: Int,
  ): Boolean

  fun close(readerHandle: Long): Boolean
}

/**
 * One independent ZIP, 7z, RAR, or tar reader owned by a positive Rust opaque handle.
 *
 * Java allocates every direct extraction buffer, so no Rust allocation or raw pointer crosses the
 * JNI call. Closing is idempotent and the Rust registry waits for an active extraction before
 * releasing the duplicated source descriptor.
 */
internal class RustArchiveReaderV2
  private constructor(
    private val readerHandle: Long,
    private val nativeApi: ArchiveReaderNativeApi,
  ) : AutoCloseable {
    private val closed = AtomicBoolean(false)

    val needsPassword: Boolean
      get() {
        checkOpen()
        return nativeApi.needsPassword(readerHandle)
      }

    val entryCount: Int
      get() {
        checkOpen()
        return nativeApi.entryCount(readerHandle).also { count ->
          check(count > 0) { "Rust archive reader returned no image entries" }
        }
      }

    val hasUnsafePath: Boolean
      get() {
        checkOpen()
        return nativeApi.hasUnsafePath(readerHandle)
      }

    fun providePassword(password: String): Boolean {
      checkOpen()
      val encodedPassword = password.encodeToByteArray()
      return try {
        nativeApi.providePassword(readerHandle, encodedPassword).also { accepted ->
          if (accepted) entryCount
        }
      } finally {
        encodedPassword.fill(0)
      }
    }

    /** Returns one filtered image entry's lowercase extension. */
    fun extension(index: Int): String {
      validateIndex(index)
      return nativeApi.extension(readerHandle, index).also { extension ->
        check(extension.isNotEmpty()) { "Rust archive reader returned an empty extension" }
      }
    }

    fun name(index: Int): String {
      validateIndex(index)
      return nativeApi.name(readerHandle, index).also { name ->
        check(name.isNotEmpty()) { "Rust archive reader returned an empty entry name" }
      }
    }

    fun size(index: Int): Long {
      validateIndex(index)
      return nativeApi.entrySize(readerHandle, index).also { size ->
        check(size > 0) { "Rust archive reader returned an invalid entry size" }
      }
    }

    fun verify(index: Int): Boolean {
      validateIndex(index)
      return nativeApi.verify(readerHandle, index)
    }

    fun recoveryMetadata(kind: RecoveryArchiveMetadataKind): ByteArray? {
      checkOpen()
      return nativeApi.recoveryMetadata(readerHandle, kind.nativeId)
    }

    /** Inspects one validated V3 batch without per-entry JNI calls. */
    fun inspectRecovery(request: ArchiveRecoveryRequest): ArchiveRecoveryBatch {
      checkOpen()
      return inspectArchiveRecovery(request) { buffer, size ->
        nativeApi.inspectRecovery(readerHandle, buffer, size)
      }
    }

    /** Extracts one image into a Java-owned direct buffer after CRC verification. */
    fun extract(index: Int): ByteBuffer {
      validateIndex(index)
      val size = nativeApi.entrySize(readerHandle, index)
      check(size in 1..Int.MAX_VALUE.toLong()) { "Rust archive entry size is outside Java range" }
      val output = ByteBuffer.allocateDirect(size.toInt())
      val extracted = nativeApi.extract(readerHandle, index, output)
      check(extracted == size.toInt()) { "Rust archive extraction size does not match metadata" }
      output.position(0)
      output.limit(extracted)
      return output
    }

    /** Streams one image into an Android-owned output descriptor after CRC verification. */
    fun extractToFileDescriptor(
      index: Int,
      fileDescriptor: Int,
    ): Boolean {
      validateIndex(index)
      require(fileDescriptor >= 0) { "Archive output file descriptor must be non-negative" }
      return nativeApi.extractToFileDescriptor(readerHandle, index, fileDescriptor)
    }

    override fun close() {
      if (closed.compareAndSet(false, true)) {
        nativeApi.close(readerHandle)
      }
    }

    private fun validateIndex(index: Int) {
      checkOpen()
      require(index in 0 until entryCount) { "Archive image index is outside the reader" }
    }

    private fun checkOpen() {
      check(!closed.get()) { "Rust archive reader is closed" }
    }

    companion object {
      /**
       * Opens one supported archive and validates its initial handle/count atomically from Kotlin's
       * view.
       *
       * A handle is closed immediately if count validation fails after native open succeeds.
       */
      fun open(
        fileDescriptor: Int,
        sortEntries: Boolean,
        nativeApi: ArchiveReaderNativeApi = JniArchiveReaderNativeApi,
      ): RustArchiveReaderV2 {
        require(fileDescriptor >= 0) { "Archive source file descriptor must be non-negative" }
        val readerHandle = nativeApi.open(fileDescriptor, sortEntries)
        check(readerHandle > 0) { "Rust archive reader returned an invalid handle" }
        return try {
          RustArchiveReaderV2(readerHandle, nativeApi).also { reader ->
            if (!reader.needsPassword) reader.entryCount
          }
        } catch (error: Throwable) {
          runCatching { nativeApi.close(readerHandle) }
          throw error
        }
      }
    }
  }

private object JniArchiveReaderNativeApi : ArchiveReaderNativeApi {
  override fun open(
    fileDescriptor: Int,
    sortEntries: Boolean,
  ) = nativeOpenArchiveV2(fileDescriptor, sortEntries)

  override fun needsPassword(readerHandle: Long) = nativeArchiveNeedsPasswordV2(readerHandle)

  override fun providePassword(
    readerHandle: Long,
    password: ByteArray,
  ) = nativeProvideArchivePasswordV2(readerHandle, password)

  override fun entryCount(readerHandle: Long) = nativeArchiveEntryCountV2(readerHandle)

  override fun entrySize(
    readerHandle: Long,
    index: Int,
  ) = nativeArchiveEntrySizeV2(readerHandle, index)

  override fun extension(
    readerHandle: Long,
    index: Int,
  ) = nativeArchiveEntryExtensionV2(readerHandle, index)

  override fun name(
    readerHandle: Long,
    index: Int,
  ) = nativeArchiveEntryNameV2(readerHandle, index)

  override fun verify(
    readerHandle: Long,
    index: Int,
  ) = nativeVerifyArchiveEntryV2(readerHandle, index)

  override fun hasUnsafePath(readerHandle: Long) = nativeArchiveHasUnsafePathV2(readerHandle)

  override fun recoveryMetadata(
    readerHandle: Long,
    kind: Int,
  ) = nativeArchiveRecoveryMetadataV2(readerHandle, kind)

  override fun inspectRecovery(
    readerHandle: Long,
    request: ByteBuffer,
    requestSize: Int,
  ) = nativeInspectArchiveRecoveryV3(readerHandle, request, requestSize)

  override fun extract(
    readerHandle: Long,
    index: Int,
    output: ByteBuffer,
  ) = nativeExtractArchiveEntryV2(readerHandle, index, output)

  override fun extractToFileDescriptor(
    readerHandle: Long,
    index: Int,
    fileDescriptor: Int,
  ) = nativeExtractArchiveEntryToFdV2(readerHandle, index, fileDescriptor)

  override fun close(readerHandle: Long) = nativeCloseArchiveV2(readerHandle)
}

/** Opens one supported archive and returns a positive opaque handle. */
private external fun nativeOpenArchiveV2(
  fileDescriptor: Int,
  sortEntries: Boolean,
): Long

private external fun nativeArchiveNeedsPasswordV2(readerHandle: Long): Boolean

private external fun nativeProvideArchivePasswordV2(
  readerHandle: Long,
  password: ByteArray,
): Boolean

/** Returns the filtered playable-image count for one handle. */
private external fun nativeArchiveEntryCountV2(readerHandle: Long): Int

/** Returns one filtered image's uncompressed size. */
private external fun nativeArchiveEntrySizeV2(
  readerHandle: Long,
  index: Int,
): Long

/** Returns one filtered image's lowercase extension. */
private external fun nativeArchiveEntryExtensionV2(
  readerHandle: Long,
  index: Int,
): String

private external fun nativeArchiveEntryNameV2(
  readerHandle: Long,
  index: Int,
): String

private external fun nativeVerifyArchiveEntryV2(
  readerHandle: Long,
  index: Int,
): Boolean

private external fun nativeArchiveHasUnsafePathV2(readerHandle: Long): Boolean

private external fun nativeArchiveRecoveryMetadataV2(
  readerHandle: Long,
  kind: Int,
): ByteArray?

/** Returns one exact-sized CBOR envelope from the Rust recovery batch owner. */
private external fun nativeInspectArchiveRecoveryV3(
  readerHandle: Long,
  request: ByteBuffer,
  requestSize: Int,
): ByteArray

/** Fills one Java-owned direct buffer and returns the CRC-verified byte count. */
private external fun nativeExtractArchiveEntryV2(
  readerHandle: Long,
  index: Int,
  output: ByteBuffer,
): Int

/** Streams one CRC-verified image into an Android-owned output descriptor. */
private external fun nativeExtractArchiveEntryToFdV2(
  readerHandle: Long,
  index: Int,
  fileDescriptor: Int,
): Boolean

/** Idempotently closes one opaque Rust archive reader handle. */
private external fun nativeCloseArchiveV2(readerHandle: Long): Boolean

internal enum class RecoveryArchiveMetadataKind(
  internal val nativeId: Int,
) {
  SPIDER_INFO(0),
  COMIC_INFO(1),
}
