package com.hippo.ehviewer.bridge.files

import java.text.DecimalFormatSymbols

/** Locale is a platform observation; Rust owns width, sign and wrapping arithmetic. */
internal fun rustPageFilename(
  index: Int,
  extension: String,
  zeroDigit: Char = DecimalFormatSymbols.getInstance().zeroDigit,
  nativeCall: (Int, Int, CharArray) -> CharArray = ::nativePageFilenameV1,
): String {
  require(extension.length <= 4096) { "Download extension exceeds limit" }
  val name = nativeCall(index, zeroDigit.code, extension.toCharArray())
  require(
    name.size in extension.length + 9..extension.length + 12 &&
      name[name.size - extension.length - 1] == '.' &&
      extension.indices.all { name[name.size - extension.length + it] == extension[it] } &&
      (0 until name.size - extension.length - 1).all {
        (it == 0 && name[it] == '-') || name[it].code in zeroDigit.code..zeroDigit.code + 9
      }
  ) {
    "Invalid Rust page filename"
  }
  return name.concatToString()
}

internal fun expectedImageHash(
  url: String,
  nativeCall: (CharArray) -> String? = ::nativeExpectedImageHashV1,
): String? {
  require(url.length <= 16 * 1024) { "Download URL exceeds limit" }
  return nativeCall(url.toCharArray()).also { hash ->
    require(hash == null || (hash.length == 40 && hash.all { it in '0'..'9' || it in 'a'..'f' })) {
      "Invalid Rust image hash"
    }
  }
}

/** ICU matching stays a platform observation; Rust selects one bounded cleanup batch. */
internal fun planPostArchivePages(
  matches: BooleanArray,
  nativeCall: (BooleanArray) -> IntArray = ::nativePostArchivePagesV1,
): IntArray {
  require(matches.size <= 100_000) { "Download cleanup exceeds limit" }
  val indices = nativeCall(matches)
  require(
    indices.size == matches.count { it } &&
      indices.indices.all {
        indices[it] in matches.indices &&
          matches[indices[it]] &&
          (it == 0 || indices[it - 1] < indices[it])
      }
  ) {
    "Invalid Rust cleanup slots"
  }
  return indices
}

private external fun nativePageFilenameV1(
  index: Int,
  zeroDigit: Int,
  extension: CharArray,
): CharArray

private external fun nativeExpectedImageHashV1(url: CharArray): String?

private external fun nativePostArchivePagesV1(matches: BooleanArray): IntArray
