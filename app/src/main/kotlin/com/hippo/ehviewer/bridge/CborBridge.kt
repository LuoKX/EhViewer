package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

/**
 * Canonical CBOR wire format shared by every Kotlin-to-Rust bridge.
 *
 * Rust golden fixtures are produced by `serde_cbor`, which uses definite-length maps and arrays.
 * Keeping that choice explicit prevents Kotlin serialization upgrades from silently changing the
 * bytes accepted by the native boundary.
 */
@OptIn(ExperimentalSerializationApi::class)
internal val rustCbor = Cbor { useDefiniteLengthEncoding = true }

/**
 * Invokes one direct-buffer Rust CBOR operation and decodes its atomic response.
 *
 * The native function must leave [request] untouched and return the number of bytes written at the
 * start of its response buffer. Capacity arithmetic and the returned byte count are validated
 * before decoding so a malformed native response never reaches a feature adapter.
 */
internal inline fun <reified Response> invokeRustCbor(
  request: ByteArray,
  operation: String,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): Response {
  val requestBuffer =
    ByteBuffer.allocateDirect(request.size).apply {
      put(request)
      flip()
    }
  val responseCapacity =
    Math.addExact(Math.multiplyExact(request.size, RESPONSE_CAPACITY_MULTIPLIER), RESPONSE_HEADROOM)
  val responseBuffer = ByteBuffer.allocateDirect(responseCapacity)
  val responseSize = nativeCall(requestBuffer, request.size, responseBuffer)
  require(responseSize in 1..responseCapacity) {
    "Rust $operation response size $responseSize exceeds capacity $responseCapacity"
  }
  val response = ByteArray(responseSize)
  responseBuffer.position(0)
  responseBuffer.get(response)
  return rustCbor.decodeFromByteArray(response)
}

private const val RESPONSE_CAPACITY_MULTIPLIER = 4
private const val RESPONSE_HEADROOM = 4096
