package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class LabelRulesBridgeTest {
  @Test
  fun editErrorsAndSettingsMenuKeepPriorityAndTwoPrefixes() {
    val names = listOf("default", "a", "A", "\ud800")
    assertEquals(1, validateLabelName(" \u2003", " \u2003", names))
    assertEquals(2, validateLabelName("default", "default", names))
    assertEquals(3, validateLabelName("a", "default", names))
    assertEquals(0, validateLabelName(" a ", "default", names))
    assertEquals(3, validateLabelName("\ud800", "default", names))
    assertEquals(0, defaultLabelSelection(false, "a", names))
    assertEquals(1, defaultLabelSelection(true, null, names))
    assertEquals(1, defaultLabelSelection(true, "missing", names))
    assertEquals(3, defaultLabelSelection(true, "a", names))
    assertEquals(DefaultLabelChoice(false, null), chooseDefaultLabel(0, names))
    assertEquals(DefaultLabelChoice(true, null), chooseDefaultLabel(1, names))
    names.forEachIndexed { index, label ->
      assertEquals(DefaultLabelChoice(true, label), chooseDefaultLabel(index + 2, names))
    }
    assertFails { chooseDefaultLabel(names.size + 2, names) }
  }

  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun sharedWireGoldenMatchesKotlinAndRust() {
    fun golden(kind: String) =
      java.io
        .File(
          UpdaterHostLibrary.root,
          "app/src/main/rust/crates/domain/test_data/label_rules_${kind}.cbor.hex",
        )
        .readText()
        .trim()
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
    val plan =
      planLabelRules(LabelAction.MATCHING, listOf("a", "A", "a", null, "\ud800"), "a") {
        buffer,
        size ->
        assertContentEquals(golden("request"), ByteArray(size).also { buffer.duplicate().get(it) })
        golden("response")
      }
    assertEquals(listOf(0, 2), plan.indices)
  }

  @Test
  fun rawNamesAndRepeatedNamesMatchJavaEqualityThroughJni() {
    val names = listOf(null, "", "a", "A", "a", "\ud800", "\udc00", "😀", "é", "é")
    for (target in names + listOf("absent")) {
      assertEquals(if (target == null) -1 else names.indexOf(target), labelIndex(names, target))
      assertEquals(
        if (target == null || target in names) -1 else names.size,
        planLabelRules(LabelAction.ADD, names, target).value,
      )
      assertEquals(
        if (target == null || target in names) 1 else 0,
        planLabelRules(LabelAction.ASSIGN, names, target).value,
      )
      assertEquals(
        names.indices.filter { names[it] == target },
        planLabelRules(LabelAction.MATCHING, names, target).indices,
      )
    }
  }

  @Test
  fun suffixUsesCurrentListSlotsAndPreservesPositionOverflow() {
    val positions = listOf(9, 9, -1, Int.MIN_VALUE, Int.MAX_VALUE)
    for (index in positions.indices) {
      val plan = planLabelRules(LabelAction.DELETE_POSITIONS, positions = positions, index = index)
      assertEquals((index + 1 until positions.size).toList(), plan.indices)
      assertEquals(positions.drop(index + 1).map { it - 1 }, plan.positions)
    }
    // A suspension may introduce new rows; the second phase observes the new snapshot.
    val names = mutableListOf("old", "other")
    assertEquals(0, labelIndex(names, "old"))
    names.add("old")
    assertEquals(listOf(0, 2), planLabelRules(LabelAction.MATCHING, names, "old").indices)
    assertFails { planLabelRules(LabelAction.DELETE_POSITIONS, positions = positions, index = -1) }
  }

  @Test
  fun wholePlanValidationRejectsInvalidSlotsPositionsAndTransport() {
    for (bad in
      listOf(
        LabelPlan(0, listOf(-1), emptyList()),
        LabelPlan(0, listOf(1), emptyList()),
        LabelPlan(0, listOf(0, 0), emptyList()),
        LabelPlan(0, listOf(0), listOf(4)),
      )) {
      assertFails {
        planLabelRules(LabelAction.MATCHING, listOf("x"), "x") { _, _ ->
          rustCbor.encodeToByteArray(ApiEnvelope(1, "labelRules", bad))
        }
      }
    }
    assertFails {
      planLabelRules(LabelAction.LOOKUP, listOf("x"), "x") { _, _ -> byteArrayOf(0xff.toByte()) }
    }
    assertFails {
      planLabelRules(LabelAction.LOOKUP, listOf("x"), "x") { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(2, "labelRules", LabelPlan(0, emptyList(), emptyList()))
        )
      }
    }
    assertFailsWith<IllegalArgumentException> {
      planLabelRules(LabelAction.ADD, name = "x".repeat(4097)) { _, _ -> fail("oversized") }
    }
    assertFailsWith<UnsatisfiedLinkError> { labelFailure() }
  }

  private fun labelFailure() =
    planLabelRules(LabelAction.ADD, name = "x") { _, _ -> throw UnsatisfiedLinkError("fixture") }
}
