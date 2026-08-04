package com.hippo.ehviewer.bridge

internal fun String.decodeHex(): ByteArray {
  require(length % 2 == 0) { "Hex fixtures must contain complete bytes" }
  return ByteArray(length / 2) { index ->
    substring(index * 2, index * 2 + 2).toInt(16).toByte()
  }
}
