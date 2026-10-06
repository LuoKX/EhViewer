package com.hippo.ehviewer.download.recovery

import com.ehviewer.core.database.model.DownloadRecoverySourceKind
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryPlan
import com.hippo.ehviewer.bridge.download.RecoveryDirectoryPromotionPlan
import com.hippo.ehviewer.bridge.download.RecoveryPathDecision
import com.hippo.ehviewer.bridge.download.planRecoveryDirectory
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertSame
import kotlinx.serialization.encodeToByteArray

class RecoveryDirectoryAndroidAdapterTest {
  @Test
  fun scanBatchUsesOneValidatedPlanAndOnlyOriginalPathPrefixes() {
    val sources =
      listOf(
        RecoveryPromotionInput("😀/42.CBZ", DownloadRecoverySourceKind.CBZ, 42),
        RecoveryPromotionInput("folder", DownloadRecoverySourceKind.DIRECTORY, 7),
        RecoveryPromotionInput("7.zip", DownloadRecoverySourceKind.ZIP, 7),
        RecoveryPromotionInput("\uD800/9.zıp", DownloadRecoverySourceKind.ZIP, 9),
      )
    var calls = 0
    val paths =
      recoveryPromotionPaths(sources) { request ->
        calls++
        assertEquals(4, request.promotions.size)
        planRecoveryDirectory(request) { _, _, output ->
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(
                1,
                "recoveryDirectory",
                payload =
                  RecoveryDirectoryPlan(
                    emptyList(),
                    null,
                    null,
                    0,
                    null,
                    listOf(
                      RecoveryDirectoryPromotionPlan(0, RecoveryPathDecision.PARENT, 2),
                      RecoveryDirectoryPromotionPlan(1, RecoveryPathDecision.RELATIVE, 6),
                      RecoveryDirectoryPromotionPlan(2, RecoveryPathDecision.NONE, 0),
                      RecoveryDirectoryPromotionPlan(3, RecoveryPathDecision.PARENT, 1),
                    ),
                  ),
              )
            )
          output.put(bytes)
          bytes.size
        }
      }
    assertEquals(1, calls)
    assertEquals(listOf("😀", "folder", null, "\uD800"), paths)
  }

  @Test
  fun badFinalSlotRejectsWholeBatchBeforeAnyPathEscapes() {
    val sources =
      listOf(
        RecoveryPromotionInput("folder", DownloadRecoverySourceKind.DIRECTORY, 7),
        RecoveryPromotionInput("other", DownloadRecoverySourceKind.DIRECTORY, 8),
      )
    assertFailsWith<IllegalArgumentException> {
      recoveryPromotionPaths(sources) { request ->
        planRecoveryDirectory(request) { _, _, output ->
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(
                1,
                "recoveryDirectory",
                payload =
                  RecoveryDirectoryPlan(
                    emptyList(),
                    null,
                    null,
                    0,
                    null,
                    listOf(
                      RecoveryDirectoryPromotionPlan(0, RecoveryPathDecision.RELATIVE, 6),
                      RecoveryDirectoryPromotionPlan(1, RecoveryPathDecision.RELATIVE, 99),
                    ),
                  ),
              )
            )
          output.put(bytes)
          bytes.size
        }
      }
    }
    val failure = UnsatisfiedLinkError("missing JNI")
    assertSame(
      failure,
      assertFailsWith<UnsatisfiedLinkError> {
        recoveryPromotionPaths(sources) { throw failure }
      },
    )
  }

  @Test
  fun everyPersistedRoomKindIncludingRar5HasClosedWireRepresentation() {
    val sources =
      DownloadRecoverySourceKind.entries.map {
        RecoveryPromotionInput("dir/42.cbz", it, 42)
      }
    val result =
      recoveryPromotionPaths(sources) { request ->
        assertEquals("RAR", request.promotions[4].kind.name)
        planRecoveryDirectory(request) { _, _, output ->
          val bytes =
            rustCbor.encodeToByteArray(
              ApiEnvelope(
                1,
                "recoveryDirectory",
                payload =
                  RecoveryDirectoryPlan(
                    emptyList(),
                    null,
                    null,
                    0,
                    null,
                    sources.indices.map {
                      RecoveryDirectoryPromotionPlan(it, RecoveryPathDecision.NONE, 0)
                    },
                  ),
              )
            )
          output.put(bytes)
          bytes.size
        }
      }
    assertEquals(List(sources.size) { null }, result)
  }

  @Test
  fun productionScanAndEhApplyShareRustAdapterWithoutArchiveChanges() {
    // Source-named routing assertion protects both callers, without needing live Room or HTTP.
    fun source(name: String): String {
      val relative = "src/main/kotlin/com/hippo/ehviewer/download/recovery/$name.kt"
      return listOf(
          java.io.File(relative),
          java.io.File("app/$relative"),
        )
        .first { it.isFile }
        .readText()
    }
    val scan = source("DownloadRecoveryCoordinator")
    assertEquals(1, Regex("recoveryPromotionPaths\\(").findAll(scan).count())
    assertEquals(0, Regex("recoveryPromotionPath\\(").findAll(scan).count())
    assertEquals(1, Regex("observeRecoveryDirectory\\(").findAll(scan).count())
    val lookup = source("DownloadRecoveryEhLookup")
    assertEquals(1, Regex("recoveryPromotionPath\\(").findAll(lookup).count())
  }
}
