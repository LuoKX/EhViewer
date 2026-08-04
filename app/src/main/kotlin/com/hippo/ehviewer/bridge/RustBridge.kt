package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

/** V1 response-envelope version owned by `eh_ffi::health`. */
internal const val RUST_CORE_API_VERSION = 1

/** Current Rust Core protocol version returned by the V1 health payload. */
internal const val RUST_CORE_PROTOCOL_VERSION = 1

/** Stable module identifier for the V1 Rust Core health envelope. */
internal const val RUST_CORE_MODULE = "rustCore"

/** Fixed, non-sensitive signed scalar used by readiness and compatibility probes. */
internal const val RUST_CORE_HEALTH_PROBE_NONCE = 0x45_48_56_31L

/** Injectable signature of the one V1 direct-buffer Rust Core health operation. */
internal typealias RustCoreHealthNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

/**
 * V1 request owned by `eh_ffi::health`.
 *
 * @property version Requested health API version; production probes send exactly `1`.
 * @property nonce Signed 64-bit scalar that Rust must echo without normalization.
 */
@Serializable internal data class RustCoreHealthRequest(val version: Int, val nonce: Long)

/**
 * Successful V1 Rust Core health payload.
 *
 * @property protocolVersion Implemented Rust Core protocol version.
 * @property nonce Signed 64-bit request nonce echoed exactly by Rust.
 */
@Serializable internal data class RustCoreHealth(val protocolVersion: Int, val nonce: Long)

/**
 * Diagnostic access to the stable V1 Rust Core health contract.
 *
 * This bridge performs no startup call, logging, network access, persistence, or feature selection.
 * Each method invokes the direct-buffer JNI operation at most once.
 */
internal object RustBridge {
  /**
   * Returns whether one recoverable V1 readiness probe validates metadata, protocol, and nonce.
   *
   * Java/Kotlin [Exception] failures and [LinkageError] return `false`; fatal non-linkage [Error]
   * values propagate unchanged.
   */
  fun isReady(nativeCall: RustCoreHealthNativeCall = ::nativeRustCoreHealthV1): Boolean =
    try {
      val health = probe(RUST_CORE_HEALTH_PROBE_NONCE, nativeCall)
      health.protocolVersion == RUST_CORE_PROTOCOL_VERSION &&
        health.nonce == RUST_CORE_HEALTH_PROBE_NONCE
    } catch (_: LinkageError) {
      false
    } catch (_: Exception) {
      false
    }

  /**
   * Strictly probes once and reports whether Rust implements [expectedVersion].
   *
   * Transport, malformed envelope, and typed Rust failures propagate. A valid response with a
   * different protocol or nonce returns `false`.
   */
  fun checkVersion(
    expectedVersion: Int = RUST_CORE_PROTOCOL_VERSION,
    nativeCall: RustCoreHealthNativeCall = ::nativeRustCoreHealthV1,
  ): Boolean {
    val health = probe(RUST_CORE_HEALTH_PROBE_NONCE, nativeCall)
    return health.protocolVersion == expectedVersion && health.nonce == RUST_CORE_HEALTH_PROBE_NONCE
  }

  /**
   * Strictly probes once and reports whether Rust echoes [nonce] exactly.
   *
   * The default is a fixed non-sensitive scalar. Transport, malformed envelope, and typed Rust
   * failures propagate; a valid response with a different nonce returns `false`.
   */
  fun ping(
    nonce: Long = RUST_CORE_HEALTH_PROBE_NONCE,
    nativeCall: RustCoreHealthNativeCall = ::nativeRustCoreHealthV1,
  ): Boolean = probe(nonce, nativeCall).nonce == nonce

  private fun probe(nonce: Long, nativeCall: RustCoreHealthNativeCall): RustCoreHealth {
    val request = RustCoreHealthRequest(version = RUST_CORE_API_VERSION, nonce = nonce)
    val envelope =
      invokeRustCbor<ApiEnvelope<RustCoreHealth>>(
        rustCbor.encodeToByteArray(request),
        "Rust Core Health",
        nativeCall,
      )
    require(envelope.version == RUST_CORE_API_VERSION) { "Unsupported Rust Core API version" }
    require(envelope.module == RUST_CORE_MODULE) {
      "Unexpected Rust Core module: ${envelope.module}"
    }
    envelope.error?.let { throw RustFailureException(it) }
    return checkNotNull(envelope.payload) { "Missing Rust Core health payload" }
  }
}

/**
 * JNI V1 Rust Core health boundary owned by `eh_ffi::jvm::health`.
 *
 * Both buffers must be direct and caller-owned. Rust reads exactly [requestLength] bytes, writes
 * one atomic CBOR envelope to [responseBuffer], retains no references, and returns the written byte
 * count. Boundary failures throw `RuntimeException`; unsupported request versions return a typed
 * envelope.
 */
private external fun nativeRustCoreHealthV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int
