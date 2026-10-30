package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import com.hippo.ehviewer.client.data.ListUrlBuilder
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class QuickSearchEditBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  private val builder = ListUrlBuilder(keyword = "query")

  @Test
  fun progressMatchesJvmLongGrammarAndKeepsFirstQueryMatch() {
    val names =
      listOf(
        "bare",
        "x@",
        "x@+",
        "x@-",
        "x@ 42",
        "x@42 ",
        "x@1@+42",
        "x@٤٢",
        "x@00042",
        "x@9223372036854775808",
        "x@-9223372036854775809",
        "x@9223372036854775807",
        "x@-9223372036854775808",
        "\ud800@-0",
        "x@-42",
      )
    for (gid in listOf(-43L, -1L, 0L, 41L, Long.MIN_VALUE, Long.MAX_VALUE - 1, Long.MAX_VALUE)) {
      val rows = names.map { builder.toQuickSearch(it) }
      val expected =
        rows
          .indexOfFirst { it.name.substringAfterLast('@', "").toLongOrNull() == gid + 1 }
          .takeIf { it >= 0 }
      val plan = checkQuickSearchCreation(builder.quickSearchSnapshot(), rows, gid)
      assertEquals(gid + 1, plan.next)
      assertEquals(expected, plan.duplicate)
    }
    val rows =
      listOf(
        builder.toQuickSearch("a@42").copy(category = 1234),
        builder.toQuickSearch("b@42"),
        builder.toQuickSearch("c@42"),
      )
    assertEquals(1, checkQuickSearchCreation(builder.quickSearchSnapshot(), rows, 41).duplicate)
    assertFalse(CatalogNumbers.quickSearchAllowed(ListUrlBuilder.MODE_IMAGE_SEARCH))
    assertTrue(CatalogNumbers.quickSearchAllowed(ListUrlBuilder.MODE_NORMAL))
  }

  @Test
  fun allBmpCharactersUseTheJvmDecimalClassification() {
    val nonDigits = mutableListOf<String>()
    for (unit in Char.MIN_VALUE..Char.MAX_VALUE) {
      val number = "${unit}2".toLongOrNull()
      if (Character.digit(unit, 10) >= 0) {
        val row = builder.toQuickSearch("name@${unit}2")
        assertEquals(
          0,
          checkQuickSearchCreation(
              builder.quickSearchSnapshot(),
              listOf(row),
              checkNotNull(number) - 1,
            )
            .duplicate,
        )
      } else nonDigits += "name@$unit"
    }
    for (chunk in nonDigits.chunked(512)) {
      assertNull(
        checkQuickSearchCreation(
            builder.quickSearchSnapshot(),
            chunk.map { builder.toQuickSearch(it) },
            -1,
          )
          .duplicate
      )
    }
  }

  @Test
  fun creationMatchesJvmTrimAndChecksNamesAfterTheDialog() {
    val inputs =
      listOf(
        "",
        " \t\n",
        "\u2003book\u00a0",
        "\u0085",
        "\u2007x\u2007",
        "x@1",
        " a ",
        "A",
        " \ud800 ",
        "\udc00",
        "😀",
      )
    for (input in inputs) for (save in listOf(false, true)) for (next in
      listOf(42L, Long.MIN_VALUE, Long.MAX_VALUE)) {
      val trimmed = input.trim()
      val plan = createQuickSearchName(input, save, next, emptyList())
      if (trimmed.isBlank()) assertEquals(QuickSearchNameError.EMPTY, plan.error)
      else {
        val expected = trimmed + if (save) "@$next" else ""
        assertNull(plan.error)
        assertEquals(expected, plan.createdName())
        assertEquals(0, plan.position)
        assertEquals(
          QuickSearchNameError.DUPLICATE,
          createQuickSearchName(input, save, next, listOf(expected)).error,
        )
      }
    }
    val names = mutableListOf("old")
    assertNull(createQuickSearchName("new", false, 42, names).error)
    names += "new"
    assertEquals(
      QuickSearchNameError.DUPLICATE,
      createQuickSearchName("new", false, 42, names).error,
    )
    assertEquals("A", createQuickSearchName("A", false, 42, listOf("a")).createdName())
  }

  @Test
  fun malformedOrIncompleteNativePlansNeverProduceCreationRows() {
    val bad =
      listOf(
        QuickSearchEditPlan(QuickSearchEditKind.CHECK, 42, null, null, null, null),
        QuickSearchEditPlan(QuickSearchEditKind.CREATE, null, null, byteArrayOf(0), 0, null),
        QuickSearchEditPlan(QuickSearchEditKind.CREATE, null, null, byteArrayOf(65, 0), 2, null),
        QuickSearchEditPlan(
          QuickSearchEditKind.CREATE,
          null,
          null,
          byteArrayOf(65, 0),
          0,
          QuickSearchNameError.EMPTY,
        ),
      )
    for (plan in bad) assertFails {
      createQuickSearchName("A", false, 42, emptyList()) { _, _ ->
        rustCbor.encodeToByteArray(ApiEnvelope(1, "quickSearchEdit", plan))
      }
    }
    assertFails {
      createQuickSearchName("A", false, 42, emptyList()) { _, _ -> byteArrayOf(0xff.toByte()) }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      createQuickSearchName("A", false, 42, emptyList()) { _, _ ->
        throw UnsatisfiedLinkError("fixture")
      }
    }
    assertFailsWith<IllegalArgumentException> {
      createQuickSearchName("x".repeat(16_385), false, 0, emptyList()) { _, _ ->
        fail("Oversized input entered JNI")
      }
    }
  }
}
