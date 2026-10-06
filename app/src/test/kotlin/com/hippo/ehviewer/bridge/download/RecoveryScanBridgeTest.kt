package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.serialization.decodeFromByteArray

class RecoveryScanBridgeTest {
  private class Fake : RecoveryScanNative {
    var opens = 0
    var closes = 0
    var handle = 7L
    var result: Long? = null
    var failOperation: Int? = null
    var closeFails = false
    val steps = mutableListOf<Pair<Int, Long>>()
    var observed = byteArrayOf()

    override fun open(request: ByteBuffer, size: Int): Long {
      opens++
      assertTrue(request.isDirect)
      val bytes = ByteArray(size).also { request.duplicate().get(it) }
      assertEquals(1, rustCbor.decodeFromByteArray<RecoveryScanOptions>(bytes).version)
      return handle
    }

    override fun step(handle: Long, operation: Int, argument: Long): Long {
      assertEquals(7L, handle)
      steps += operation to argument
      if (operation == 6) {
        closes++
        if (closeFails) error("close fixture")
        return 0
      }
      if (failOperation == operation) error("step fixture")
      return result
        ?: when (operation) {
          1,
          3 -> (1L shl 32) or 1
          2 -> 1
          else -> 0
        }
    }

    override fun observe(handle: Long, identity: ByteBuffer, size: Int): Long {
      assertEquals(7L, handle)
      assertTrue(identity.isDirect)
      observed = ByteArray(size).also { identity.duplicate().get(it) }
      return result ?: 1
    }
  }

  @Test
  fun sharedOpenFixturesUseExactBytesAndReleaseOwnedHandles() {
    for (name in listOf("physical", "documents")) {
      val bytes = golden(name)
      val request = rustCbor.decodeFromByteArray<RecoveryScanOptions>(bytes)
      val fake = Fake()
      val native =
        object : RecoveryScanNative by fake {
          override fun open(request: ByteBuffer, size: Int): Long {
            assertContentEquals(bytes, ByteArray(size).also { request.duplicate().get(it) })
            return fake.open(request, size)
          }
        }
      RecoveryScanSession.open(request, native).use {}
      assertEquals(1, fake.opens)
      assertEquals(1, fake.closes)
    }
  }

  @Test
  fun scalarStepsAndReusableUtf16IdentityBufferPreserveWireFlags() {
    val fake = Fake()
    RecoveryScanSession.open(options(), fake).use { scan ->
      assertEquals(RecoveryScanTicket(1, 1), scan.poll(10))
      assertTrue(scan.gate(11))
      assertEquals(RecoveryScanTicket(1, 1), scan.accept(true, true))
      scan.finishDirectory()
      assertFalse(scan.limitExceeded)
      scan.stop()
      assertTrue(scan.observeDocument("\uD800\uDC00\u0100"))
      assertContentEquals("\uD800\uDC00\u0100".toUtf16LeBytes(), fake.observed)
      val large = "x".repeat(300)
      assertTrue(scan.observeDocument(large))
      assertContentEquals(large.toUtf16LeBytes(), fake.observed)
      assertTrue(scan.observeDocument(""))
      assertContentEquals(byteArrayOf(), fake.observed)
    }
    assertEquals(
      listOf(1 to 10L, 2 to 11L, 3 to 3L, 4 to 0L, 5 to 0L, 7 to 0L, 6 to 0L),
      fake.steps,
    )
  }

  @Test
  fun oversizedProviderIdentitiesStopInsteadOfAllocatingUnboundedDirectBuffers() {
    val fake = Fake()
    RecoveryScanSession.open(options(), fake).use { scan ->
      assertFalse(scan.observeDocument("x".repeat(65_537)))
      assertEquals(listOf(7 to 0L), fake.steps)
      assertContentEquals(byteArrayOf(), fake.observed)
    }
  }

  @Test
  fun scopeClosesForCancellationAndFailureAndPreservesPrimaryException() {
    val fake = Fake()
    val cancelled = CancellationException("fixture")
    val actual =
      assertFailsWith<CancellationException> {
        RecoveryScanSession.open(options(), fake).use { throw cancelled }
      }
    assertSame(cancelled, actual)
    assertEquals(1, fake.closes)
    val broken = Fake().apply { failOperation = 1 }
    assertFailsWith<IllegalStateException> {
      RecoveryScanSession.open(options(), broken).use { it.poll(0) }
    }
    assertEquals(1, broken.closes)
    val closeBroken = Fake().apply { closeFails = true }
    assertSame(
      cancelled,
      assertFailsWith<CancellationException> {
        RecoveryScanSession.open(options(), closeBroken).use { throw cancelled }
      },
    )
    assertEquals(1, closeBroken.closes)
    assertEquals("close fixture", cancelled.suppressed.single().message)
  }

  @Test
  fun closeIsIdempotentAndClosedHandlesNeverReachNative() {
    val fake = Fake()
    val scan = RecoveryScanSession.open(options(), fake)
    scan.close()
    scan.close()
    assertEquals(1, fake.closes)
    assertFailsWith<IllegalStateException> { scan.poll(0) }
    assertFailsWith<IllegalStateException> { scan.observeDocument("x") }
    assertFailsWith<IllegalStateException> { scan.accept(false) }
    assertEquals(1, fake.steps.size)
  }

  @Test
  fun malformedNativeTicketsBooleansAndAcknowledgementsAreRejected() {
    for (value in
      listOf(
        -2L,
        1L,
        1L shl 32,
        (1L shl 32) or 2,
        100_000L shl 32,
        (1L shl 32) or 100_001,
        (1L shl 32) or 0xffff_ffffL,
      )) {
      val fake = Fake().apply { result = value }
      RecoveryScanSession.open(options(), fake).use { scan ->
        assertFailsWith<IllegalArgumentException> { scan.poll(0) }
      }
    }
    val fake = Fake().apply { result = 2 }
    RecoveryScanSession.open(options(), fake).use { scan ->
      assertFailsWith<IllegalArgumentException> { scan.gate(0) }
      assertFailsWith<IllegalArgumentException> { scan.observeDocument("x") }
      assertFailsWith<IllegalArgumentException> { scan.finishDirectory() }
    }
    val done = Fake().apply { result = -1 }
    RecoveryScanSession.open(options(), done).use { assertNull(it.poll(0)) }
    val skip = Fake().apply { result = -1 }
    RecoveryScanSession.open(options(), skip).use { assertNull(it.accept(false)) }
  }

  @Test
  fun invalidOpenInputsAndHandlesNeverGainOwnedSessions() {
    val fake = Fake()
    for (request in
      listOf(
        options().copy(version = 2),
        options().copy(maxNodes = 100_001),
        options().copy(rootDocumentId = byteArrayOf(1)),
        options().copy(rootDocumentId = ByteArray(131_074)),
        options().copy(backend = RecoveryScanBackend.PHYSICAL, rootDocumentId = byteArrayOf(0, 0)),
      )) {
      assertFailsWith<IllegalArgumentException> { RecoveryScanSession.open(request, fake) }
    }
    assertEquals(0, fake.opens)
    for (handle in listOf(0L, -1L)) {
      fake.handle = handle
      assertFailsWith<IllegalStateException> { RecoveryScanSession.open(options(), fake) }
    }
    assertEquals(0, fake.closes)
    val unavailable =
      object : RecoveryScanNative by fake {
        override fun open(request: ByteBuffer, size: Int): Long =
          throw UnsatisfiedLinkError("fixture")
      }
    assertFailsWith<UnsatisfiedLinkError> { RecoveryScanSession.open(options(), unavailable) }
  }

  private fun options() =
    RecoveryScanOptions(
      backend = RecoveryScanBackend.DOCUMENTS,
      maxDepth = 32,
      maxNodes = 100_000,
      startedAtMillis = 0,
      maxDurationMillis = 100,
      rootDocumentId = byteArrayOf(),
    )

  private fun golden(name: String) =
    File("src/main/rust/crates/domain/test_data/recovery_scan_${name}_open.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()
}
