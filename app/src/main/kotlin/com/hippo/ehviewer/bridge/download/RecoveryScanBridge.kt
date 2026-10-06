@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.rustCbor
import java.io.Closeable
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

@Serializable
internal enum class RecoveryScanBackend {
  @SerialName("physical") PHYSICAL,
  @SerialName("documents") DOCUMENTS,
}

@Serializable
internal data class RecoveryScanOptions(
  @EncodeDefault val version: Int = 1,
  val backend: RecoveryScanBackend,
  val maxDepth: Int,
  val maxNodes: Int,
  val startedAtMillis: Long,
  val maxDurationMillis: Long,
  @ByteString val rootDocumentId: ByteArray,
)

internal data class RecoveryScanTicket(val index: Int, val depth: Int)

internal interface RecoveryScanNative {
  fun open(request: ByteBuffer, size: Int): Long

  fun step(handle: Long, operation: Int, argument: Long): Long

  fun observe(handle: Long, identity: ByteBuffer, size: Int): Long
}

private object JniRecoveryScan : RecoveryScanNative {
  override fun open(request: ByteBuffer, size: Int) = nativeOpenRecoveryScanV1(request, size)

  override fun step(handle: Long, operation: Int, argument: Long) =
    nativeStepRecoveryScanV1(handle, operation, argument)

  override fun observe(handle: Long, identity: ByteBuffer, size: Int) =
    nativeObserveRecoveryDocumentV1(handle, identity, size)
}

/** Scoped owner: use with `use`; no retry, Android queue, seen-set or fallback policy. */
internal class RecoveryScanSession
private constructor(
  private var handle: Long,
  private val native: RecoveryScanNative,
) : Closeable {
  private var identityBuffer = ByteBuffer.allocateDirect(256).order(ByteOrder.LITTLE_ENDIAN)

  fun poll(now: Long): RecoveryScanTicket? = ticket(step(1, now))

  fun gate(now: Long): Boolean = boolean(step(2, now))

  fun observeDocument(id: String): Boolean {
    checkOpen()
    if (id.length > 65_536) {
      stop()
      return false
    }
    val size = id.length * 2
    if (identityBuffer.capacity() < size) {
      identityBuffer = ByteBuffer.allocateDirect(size).order(ByteOrder.LITTLE_ENDIAN)
    }
    identityBuffer.clear()
    for (unit in id) identityBuffer.putChar(unit)
    identityBuffer.flip()
    return boolean(native.observe(handle, identityBuffer, size))
  }

  fun accept(directory: Boolean, symbolicLink: Boolean = false): RecoveryScanTicket? =
    ticket(step(3, (if (directory) 1L else 0L) or (if (symbolicLink) 2L else 0L)))

  fun finishDirectory() = unit(step(4))

  val limitExceeded: Boolean
    get() = boolean(step(5))

  fun stop() = unit(step(7))

  override fun close() {
    if (handle == 0L) return
    val owned = handle
    handle = 0
    unit(native.step(owned, 6, 0))
  }

  private fun step(operation: Int, argument: Long = 0): Long {
    checkOpen()
    return native.step(handle, operation, argument)
  }

  private fun checkOpen() = check(handle > 0) { "Recovery scan is closed" }

  private fun ticket(value: Long): RecoveryScanTicket? {
    if (value == -1L) return null
    require(value >= 0) { "Invalid recovery scan ticket" }
    val index = (value ushr 32).toInt()
    val depth = value.toInt()
    require(index in 0 until 100_000 && depth in 0..index && ((index == 0) == (depth == 0))) {
      "Invalid recovery scan ticket"
    }
    return RecoveryScanTicket(index, depth)
  }

  private fun boolean(value: Long): Boolean {
    require(value == 0L || value == 1L) { "Invalid recovery scan boolean" }
    return value == 1L
  }

  private fun unit(value: Long) {
    require(value == 0L) { "Invalid recovery scan acknowledgement" }
  }

  companion object {
    fun open(
      request: RecoveryScanOptions,
      native: RecoveryScanNative = JniRecoveryScan,
    ): RecoveryScanSession {
      require(request.version == 1 && request.maxNodes <= 100_000) {
        "Invalid recovery scan configuration"
      }
      require(request.rootDocumentId.size <= 131_072 && request.rootDocumentId.size % 2 == 0) {
        "Invalid recovery document identity"
      }
      require(request.backend != RecoveryScanBackend.PHYSICAL || request.rootDocumentId.isEmpty()) {
        "Physical recovery scan has a provider identity"
      }
      val bytes = rustCbor.encodeToByteArray(request)
      require(bytes.size in 1..262_144) { "Invalid recovery scan request size" }
      val input =
        ByteBuffer.allocateDirect(bytes.size).apply {
          put(bytes)
          flip()
        }
      val handle = native.open(input, bytes.size)
      check(handle > 0) { "Invalid recovery scan handle" }
      return RecoveryScanSession(handle, native)
    }
  }
}

private external fun nativeOpenRecoveryScanV1(request: ByteBuffer, size: Int): Long

private external fun nativeStepRecoveryScanV1(handle: Long, operation: Int, argument: Long): Long

private external fun nativeObserveRecoveryDocumentV1(
  handle: Long,
  identity: ByteBuffer,
  size: Int,
): Long
