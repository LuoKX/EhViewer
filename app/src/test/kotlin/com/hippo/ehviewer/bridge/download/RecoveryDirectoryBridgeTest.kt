package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.RecoveryFileKind
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlin.test.assertTrue
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class RecoveryDirectoryBridgeTest {
  // Rust always emits both optional envelope fields. Test-only DTO also allows
  // malformed envelopes past constructor invariants to exercise decoder rejection.
  @Serializable
  private data class WireEnvelope(
    val version: Int,
    val module: String,
    val payload: RecoveryDirectoryPlan?,
    val error: RustError?,
  )

  private fun child(
    id: String,
    name: String,
    size: Long?,
    modified: Long?,
  ) = RecoveryDirectoryChild(id.toUtf16LeBytes(), name.toUtf16LeBytes(), size, modified)

  private val observe =
    RecoveryDirectoryRequest(
      1,
      RecoveryDirectoryOperation.OBSERVE,
      listOf(
        child("b", "ComicInfo.xml", 3, -2),
        child("a", ".EHVIEWER", 4, 5),
        child("a", ".ehviewer", null, null),
      ),
      emptyList(),
    )
  private val observed = RecoveryDirectoryPlan(listOf(1, 2, 0), 1, 0, 7, 5, emptyList())
  private val promote =
    RecoveryDirectoryRequest(
      1,
      RecoveryDirectoryOperation.PROMOTE,
      emptyList(),
      listOf(
        RecoveryDirectoryPromotion("dir/42.CBZ".toUtf16LeBytes(), RecoveryFileKind.CBZ, 42),
        RecoveryDirectoryPromotion("folder".toUtf16LeBytes(), RecoveryFileKind.DIRECTORY, 7),
        RecoveryDirectoryPromotion("7.zip".toUtf16LeBytes(), RecoveryFileKind.ZIP, 7),
      ),
    )
  private val promoted =
    RecoveryDirectoryPlan(
      emptyList(),
      null,
      null,
      0,
      null,
      listOf(
        RecoveryDirectoryPromotionPlan(0, RecoveryPathDecision.PARENT, 3),
        RecoveryDirectoryPromotionPlan(1, RecoveryPathDecision.RELATIVE, 6),
        RecoveryDirectoryPromotionPlan(2, RecoveryPathDecision.NONE, 0),
      ),
    )

  @Test
  fun independentExactRequestAndEnvelopeGoldens() {
    for ((name, request, payload) in
      listOf(
        Triple("observe", observe, observed),
        Triple("promote", promote, promoted),
      )) {
      assertContentEquals(golden(name, "request"), rustCbor.encodeToByteArray(request))
      val encoded = golden(name, "response")
      val envelope = rustCbor.decodeFromByteArray<ApiEnvelope<RecoveryDirectoryPlan>>(encoded)
      assertEquals(payload, envelope.payload)
      assertEquals(1, envelope.version)
      assertEquals("recoveryDirectory", envelope.module)
      assertEquals(null, envelope.error)
      // Client does not emit response envelopes; pin pure plan bytes independently.
      assertContentEquals(golden(name, "plan"), rustCbor.encodeToByteArray(payload))
      assertEquals(
        payload,
        rustCbor.decodeFromByteArray<RecoveryDirectoryPlan>(golden(name, "plan")),
      )
      var calls = 0
      assertEquals(
        payload,
        planRecoveryDirectory(request) { input, size, output ->
          calls++
          assertTrue(input.isDirect && output.isDirect)
          assertEquals(size, input.remaining())
          assertContentEquals(
            golden(name, "request"),
            ByteArray(size).also { input.duplicate().get(it) },
          )
          output.put(encoded)
          encoded.size
        },
      )
      assertEquals(1, calls)
    }
  }

  @Test
  fun rejectsBadRequestsBeforeNativeCalls() {
    val invalid =
      listOf(
        observe.copy(version = 2),
        observe.copy(promotions = promote.promotions),
        promote.copy(children = observe.children),
        observe.copy(children = List(100_001) { observe.children.first() }),
        promote.copy(promotions = List(100_001) { promote.promotions.first() }),
        observe.copy(children = listOf(observe.children.first().copy(nameUtf16 = byteArrayOf(1)))),
        observe.copy(
          children =
            listOf(observe.children.first().copy(sourceIdUtf16 = ByteArray(16 * 1024 * 1024)))
        ),
      )
    invalid.forEach { request ->
      assertFailsWith<IllegalArgumentException> {
        planRecoveryDirectory(request) { _, _, _ -> error("Invalid facts reached JNI") }
      }
    }
  }

  @Test
  fun signedFingerprintFactsAndOverflowProjectionStayUnnormalized() {
    val request =
      observe.copy(
        children =
          listOf(
            child("a", "page", Long.MAX_VALUE, -10),
            child("b", "page", 1, Long.MIN_VALUE),
            child("c", "page", null, null),
          )
      )
    val plan = RecoveryDirectoryPlan(listOf(0, 1, 2), null, null, Long.MIN_VALUE, -10, emptyList())
    assertEquals(plan, planRecoveryDirectory(request, native(plan)))
    val negative = request.copy(children = listOf(child("a", "page", -1, 0)))
    val negativePlan = plan.copy(indices = listOf(0), totalSize = -1, modifiedAtMillis = 0)
    assertEquals(negativePlan, planRecoveryDirectory(negative, native(negativePlan)))
  }

  @Test
  fun rejectsIncompleteDuplicateOutOfRangeAndUnrelatedSidecarPlans() {
    val invalid =
      listOf(
        observed.copy(indices = listOf(1, 0)),
        observed.copy(indices = listOf(1, 1, 0)),
        observed.copy(indices = listOf(1, 2, 3)),
        observed.copy(spiderIndex = -1),
        observed.copy(comicIndex = 3),
        observed.copy(spiderIndex = 0),
        observed.copy(comicIndex = 1),
        observed.copy(modifiedAtMillis = null),
        observed.copy(modifiedAtMillis = 123),
        observed.copy(promotions = promoted.promotions),
      )
    invalid.forEach { bad ->
      assertFailsWith<IllegalArgumentException> { planRecoveryDirectory(observe, native(bad)) }
    }
    val empty = observe.copy(children = emptyList())
    assertEquals(
      0,
      planRecoveryDirectory(
          empty,
          native(
            observed.copy(
              indices = emptyList(),
              spiderIndex = null,
              comicIndex = null,
              totalSize = 0,
              modifiedAtMillis = null,
            )
          ),
        )
        .indices
        .size,
    )
  }

  @Test
  fun rejectsWholePromotionPlanBeforePathsAreReturned() {
    val invalid =
      listOf(
        promoted.copy(indices = listOf(0)),
        promoted.copy(spiderIndex = 0),
        promoted.copy(comicIndex = 0),
        promoted.copy(totalSize = 1),
        promoted.copy(modifiedAtMillis = 1),
        promoted.copy(promotions = promoted.promotions.take(2)),
        promoted.copy(promotions = promoted.promotions.map { it.copy(index = 0) }),
        promoted.copy(promotions = promoted.promotions.map { it.copy(end = -1) }),
        promoted.copy(promotions = promoted.promotions.map { it.copy(end = 100) }),
        promoted.copy(
          promotions =
            promoted.promotions.toMutableList().apply {
              this[0] = this[0].copy(end = 2)
            }
        ),
        promoted.copy(
          promotions =
            promoted.promotions.toMutableList().apply {
              this[1] = this[1].copy(end = 5)
            }
        ),
        promoted.copy(
          promotions =
            promoted.promotions.toMutableList().apply {
              this[2] = this[2].copy(end = 1)
            }
        ),
      )
    invalid.forEach { bad ->
      assertFailsWith<IllegalArgumentException> { planRecoveryDirectory(promote, native(bad)) }
    }
    for (path in listOf("dir/43.CBZ", "dir/a/42.CBZ", "   /42.CBZ")) {
      assertFailsWith<IllegalArgumentException> {
        planRecoveryDirectory(
          promote.copy(
            promotions =
              listOf(promote.promotions[0].copy(relativePathUtf16 = path.toUtf16LeBytes()))
          ),
          native(promoted.copy(promotions = promoted.promotions.take(1))),
        )
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planRecoveryDirectory(
        promote.copy(
          promotions =
            promote.promotions.map {
              it.copy(gid = 0)
            }
        ),
        native(promoted),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      planRecoveryDirectory(
        promote.copy(
          promotions =
            promote.promotions.map {
              it.copy(kind = RecoveryFileKind.RAR)
            }
        ),
        native(promoted),
      )
    }
  }

  @Test
  fun malformedEnvelopeUnknownEnumSizeAndRedaction() {
    for (bytes in
      listOf(
        byteArrayOf(0),
        byteArrayOf(),
        response(observed, 2),
        response(observed, module = "secret-path"),
      )) {
      val error =
        assertFailsWith<Exception> {
          planRecoveryDirectory(observe) { _, _, output ->
            output.put(bytes)
            bytes.size
          }
        }
      assertTrue(!error.message.orEmpty().contains("secret-path"))
    }
    for (size in listOf(0, -1, 1_000_000)) {
      assertFailsWith<IllegalArgumentException> {
        planRecoveryDirectory(observe) { _, _, _ -> size }
      }
    }
    val bytes = response(promoted)
    val text =
      bytes
        .toString(Charsets.ISO_8859_1)
        .replace("parent", "foobar")
        .toByteArray(Charsets.ISO_8859_1)
    val enumError =
      assertFailsWith<IllegalArgumentException> {
        planRecoveryDirectory(promote) { _, _, output ->
          output.put(text)
          text.size
        }
      }
    assertEquals("Invalid recovery directory response", enumError.message)
    assertEquals(null, enumError.cause)
    val ambiguous =
      rustCbor.encodeToByteArray(
        WireEnvelope(
          1,
          "recoveryDirectory",
          payload = observed,
          error = RustError(RustErrorKind.ERROR, "invalid request"),
        )
      )
    assertFailsWith<IllegalArgumentException> {
      planRecoveryDirectory(observe) { _, _, output ->
        output.put(ambiguous)
        ambiguous.size
      }
    }
    val missing = rustCbor.encodeToByteArray(WireEnvelope(1, "recoveryDirectory", null, null))
    assertFailsWith<IllegalArgumentException> {
      planRecoveryDirectory(observe) { _, _, output ->
        output.put(missing)
        missing.size
      }
    }
  }

  @Test
  fun nativeAndTypedFailureSurfaceWithoutFallback() {
    val failure = UnsatisfiedLinkError("missing JNI")
    var calls = 0
    assertSame(
      failure,
      assertFailsWith<UnsatisfiedLinkError> {
        planRecoveryDirectory(observe) { _, _, _ ->
          calls++
          throw failure
        }
      },
    )
    assertEquals(1, calls)
    val bytes =
      rustCbor.encodeToByteArray(
        ApiEnvelope<RecoveryDirectoryPlan>(
          1,
          "recoveryDirectory",
          error = RustError(RustErrorKind.ERROR, "Invalid recovery directory request"),
        )
      )
    assertFailsWith<RustFailureException> {
      planRecoveryDirectory(observe) { _, _, output ->
        output.put(bytes)
        bytes.size
      }
    }
  }

  private fun native(payload: RecoveryDirectoryPlan): (ByteBuffer, Int, ByteBuffer) -> Int =
    { _, _, output ->
      val bytes = response(payload)
      output.put(bytes)
      bytes.size
    }

  private fun response(
    payload: RecoveryDirectoryPlan,
    version: Int = 1,
    module: String = "recoveryDirectory",
  ) = rustCbor.encodeToByteArray(ApiEnvelope(version, module, payload = payload))

  private fun golden(
    operation: String,
    kind: String,
  ): ByteArray {
    val relative =
      "src/main/rust/crates/domain/test_data/recovery_directory_${operation}_$kind.cbor.hex"
    val hex = listOf(File(relative), File("app/$relative")).first { it.isFile }.readText().trim()
    return ByteArray(hex.length / 2) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
  }
}
