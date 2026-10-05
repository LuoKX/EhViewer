package com.hippo.ehviewer.bridge.download

/** Scalar V1 transport injection; no speed calculations or fallback live in Android. */
internal class TransferSpeedNative(
  val open: (Int, Long) -> Long = ::nativeOpenTransferSpeedV1,
  val update: (Long, Int, Long, Int) -> Long = ::nativeUpdateTransferSpeedV1,
  val close: (Long) -> Unit = ::nativeCloseTransferSpeedV1,
)

/** Owns one `eh_domain::transfer_speed` window; synchronous operations serialize with close. */
internal class TransferSpeedHandle(
  windowNanos: Long,
  private val native: TransferSpeedNative = TransferSpeedNative(),
) : AutoCloseable {
  private var handle =
    native.open(1, windowNanos).also { require(it > 0) { "Invalid transfer speed handle" } }

  @Synchronized
  fun reset() {
    command(0, 0, 0).also { require(it == 0L) { "Invalid transfer reset result" } }
  }

  @Synchronized
  fun track(now: Long, bytes: Int) {
    command(1, now, bytes).also { require(it == 0L) { "Invalid transfer tracking result" } }
  }

  @Synchronized
  fun sample(now: Long): Long? =
    command(2, now, 0)
      .also {
        require(it >= -1) { "Invalid transfer sample result" }
      }
      .takeUnless { it == -1L }

  private fun command(operation: Int, now: Long, bytes: Int): Long {
    check(handle > 0) { "Transfer speed handle is closed" }
    return native.update(handle, operation, now, bytes)
  }

  @Synchronized
  override fun close() {
    if (handle != 0L) {
      native.close(handle)
      handle = 0L
    }
  }
}

/** Creates one native-owned V1 window. */
private external fun nativeOpenTransferSpeedV1(version: Int, windowNanos: Long): Long

/** Commands: zero reset, one track, two sample. Minus one means no sample yet. */
private external fun nativeUpdateTransferSpeedV1(
  handle: Long,
  operation: Int,
  now: Long,
  bytes: Int,
): Long

/** Retires V1 ownership; native close is idempotent. */
private external fun nativeCloseTransferSpeedV1(handle: Long)
