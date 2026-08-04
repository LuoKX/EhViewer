package com.hippo.ehviewer.bridge.archive

import com.ehviewer.core.files.openFileDescriptor
import okio.Path

/**
 * Validates one completed application-generated CBZ through the Rust archive V1 contract.
 *
 * The descriptor stays owned by Android and remains open for the complete synchronous JNI call. The
 * returned value is the total number of uncompressed bytes whose ZIP CRC was verified.
 */
internal fun validateDownloadedCbzV1(file: Path, expectedImages: Int): Long =
  file.openFileDescriptor("r").use { descriptor ->
    validateDownloadedCbzDescriptorV1(descriptor.fd, expectedImages)
  }

/**
 * Validates descriptor inputs and invokes the selected native implementation exactly once.
 *
 * [nativeCall] is injectable so JVM tests can verify the boundary without loading Android native
 * libraries.
 */
internal fun validateDownloadedCbzDescriptorV1(
  fileDescriptor: Int,
  expectedImages: Int,
  nativeCall: (fileDescriptor: Int, expectedImages: Int) -> Long = ::nativeValidateDownloadedCbzV1,
): Long {
  require(fileDescriptor >= 0) { "Downloaded CBZ file descriptor must be non-negative" }
  require(expectedImages > 0) { "Downloaded CBZ expected image count must be positive" }
  return nativeCall(fileDescriptor, expectedImages).also { validatedBytes ->
    check(validatedBytes > 0) { "Rust archive validator returned an invalid byte count" }
  }
}

/** Invokes the Rust V1 validator for an Android-owned archive descriptor. */
private external fun nativeValidateDownloadedCbzV1(
  fileDescriptor: Int,
  expectedImages: Int,
): Long
