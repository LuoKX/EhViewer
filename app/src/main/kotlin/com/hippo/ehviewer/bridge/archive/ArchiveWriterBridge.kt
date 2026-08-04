package com.hippo.ehviewer.bridge.archive

/**
 * Executes exactly one generated-CBZ writer for a default-off rollout.
 *
 * Exceptions propagate directly. In particular, a Rust failure never invokes [legacyWrite] on the
 * potentially partial output.
 */
internal fun writeGeneratedCbzWithSelectedWriter(
  useRustWriter: Boolean,
  rustWrite: () -> Unit,
  legacyWrite: () -> Unit,
) {
  if (useRustWriter) {
    rustWrite()
  } else {
    legacyWrite()
  }
}

/**
 * Writes one application-generated CBZ through the Rust archive V1 contract.
 *
 * Every descriptor remains Android-owned and open for the complete synchronous JNI call. Source
 * descriptors are freshly opened by the caller and consumed to EOF. The output descriptor must be
 * opened at position zero with truncation enabled. A failure must discard the output instead of
 * invoking another writer on the same descriptor.
 *
 * [nativeCall] is injectable so JVM tests can verify the boundary without loading Android native
 * libraries. The returned value is the total number of uncompressed bytes written.
 */
internal fun writeGeneratedCbzDescriptorV1(
  sourceDescriptors: IntArray,
  entryNames: Array<String>,
  outputDescriptor: Int,
  expectedImages: Int,
  nativeCall:
    (
      sourceDescriptors: IntArray,
      entryNames: Array<String>,
      outputDescriptor: Int,
      expectedImages: Int,
    ) -> Long =
    ::nativeWriteGeneratedCbzV1,
): Long {
  require(outputDescriptor >= 0) { "Generated CBZ output descriptor must be non-negative" }
  require(expectedImages > 0) { "Generated CBZ expected image count must be positive" }
  require(expectedImages <= MAX_GENERATED_CBZ_IMAGES) {
    "Generated CBZ expected image count exceeds the eight-digit page namespace"
  }
  val expectedEntries = expectedImages + 1
  require(sourceDescriptors.size == expectedEntries) {
    "Generated CBZ source descriptor count must match pages plus metadata"
  }
  require(entryNames.size == expectedEntries) {
    "Generated CBZ entry name count must match pages plus metadata"
  }
  require(sourceDescriptors.all { it >= 0 }) {
    "Generated CBZ source descriptors must be non-negative"
  }

  return nativeCall(sourceDescriptors, entryNames, outputDescriptor, expectedImages).also {
    writtenBytes ->
    check(writtenBytes > 0) { "Rust archive writer returned an invalid byte count" }
  }
}

/** Invokes the Rust V1 writer for Android-owned source and output descriptors. */
private external fun nativeWriteGeneratedCbzV1(
  sourceDescriptors: IntArray,
  entryNames: Array<String>,
  outputDescriptor: Int,
  expectedImages: Int,
): Long

private const val MAX_GENERATED_CBZ_IMAGES = 99_999_999
