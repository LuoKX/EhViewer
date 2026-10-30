package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.download.LabelAction
import com.hippo.ehviewer.bridge.download.planLabelRules
import com.hippo.ehviewer.bridge.library.ListOrderRequest
import com.hippo.ehviewer.bridge.library.planListOrder
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import java.io.File
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class CatalogEditWireTest {
  @Test
  fun bothQuickSearchPhasesShareExactCborWithRustAndJava() {
    for (name in listOf("quick_search_check", "quick_search_create")) {
      val bytes = golden(name, "request")
      val request = rustCbor.decodeFromByteArray<QuickSearchEditRequest>(bytes)
      assertContentEquals(bytes, rustCbor.encodeToByteArray(request))
      val plan = planQuickSearchEdit(request, boundary(name))
      if (request.kind == QuickSearchEditKind.CHECK) {
        assertEquals(42L, plan.next)
        assertEquals(1, plan.duplicate)
      } else {
        assertEquals("X\ud800@42", plan.createdName())
        assertEquals(1, plan.position)
      }
    }
  }

  @Test
  fun positionalAndLabelEditorFieldsKeepExactProtocolDefaults() {
    val request =
      rustCbor.decodeFromByteArray<ListOrderRequest>(golden("list_order_finish", "request"))
    assertContentEquals(golden("list_order_finish", "request"), rustCbor.encodeToByteArray(request))
    val order =
      planListOrder(
        request.action,
        request.count,
        request.from,
        request.to,
        request.headers,
        request.positions,
        boundary("list_order_finish"),
      )
    assertEquals(listOf(1, 2, 3), order.indices)
    assertEquals(
      2,
      planLabelRules(
          LabelAction.VALIDATE_NAME,
          listOf("default"),
          "default",
          blank = false,
          reserved = "default",
          nativeCall = boundary("label_edit"),
        )
        .value,
    )
  }

  private fun golden(name: String, kind: String) =
    File(
        UpdaterHostLibrary.root,
        "app/src/main/rust/crates/domain/test_data/${name}_${kind}.cbor.hex",
      )
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun boundary(name: String): (ByteBuffer, Int) -> ByteArray = { input, size ->
    assertTrue(input.isDirect)
    assertContentEquals(golden(name, "request"), ByteArray(size).also { input.duplicate().get(it) })
    golden(name, "response")
  }
}
