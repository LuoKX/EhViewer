package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import com.hippo.ehviewer.client.data.ListUrlBuilder
import java.nio.ByteBuffer
import kotlin.test.*
import kotlinx.serialization.encodeToByteArray

class CatalogStateBridgeTest {
  @Test
  fun favoritesButtonsReturnNewBuildersAndKeepTheFavoriteLastCursor() {
    val original =
      com.hippo.ehviewer.client.data.FavListUrlBuilder(
        favCat = 3,
        keyword = "query",
        jumpTo = "old",
        prev = "prev",
        next = "next",
      )
    val seek = original.seek("\ud800-date")
    assertNotSame(original, seek)
    assertEquals(original.copy(jumpTo = "\ud800-date", prev = null, next = "2"), seek)
    assertEquals(original.copy(jumpTo = null, prev = "1-0", next = null), original.lastPage())
    assertEquals(
      com.hippo.ehviewer.client.data.FavListUrlBuilder(3, "query", "old", "prev", "next"),
      original,
    )
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
          "app/src/main/rust/crates/domain/test_data/catalog_state_${kind}.cbor.hex",
        )
        .readText()
        .trim()
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
    val input = golden("request")
    val plan =
      transitionCatalog(
        CatalogNavigation.from("p", "n", null, 23),
        CatalogTransition.PAGE,
        value = Int.MIN_VALUE,
      ) { buffer, size ->
        assertContentEquals(input, ByteArray(size).also { buffer.duplicate().get(it) })
        golden("response")
      }
    assertEquals("2147483647", plan.seek?.catalogText())
    assertEquals("p", plan.previous?.catalogText())
  }

  @Test
  fun numericRulesDifferentialAgainstOriginalJvmArithmetic() {
    val edges =
      listOf(
        Int.MIN_VALUE,
        Int.MIN_VALUE + 1,
        -2001,
        -20,
        -2,
        -1,
        0,
        1,
        2,
        9,
        10,
        19,
        20,
        21,
        40,
        199,
        200,
        201,
        999,
        1000,
        1001,
        1999,
        2000,
        2001,
        Int.MAX_VALUE,
      )
    for (a in edges) {
      assertEquals((a - 1).coerceAtLeast(0), CatalogNumbers.ratingIndex(a))
      assertEquals(if (a == 0) 0 else a + 1, CatalogNumbers.ratingValue(a))
      assertEquals(a.coerceIn(0, 1000), CatalogNumbers.minimumPages(a))
      assertEquals(a.coerceIn(0, 2000), CatalogNumbers.maximumPages(a))
      assertEquals(a in 1..200, CatalogNumbers.validToplistPage(a))
      assertEquals(
        (a - 1).takeIf { it > 0 } to (a + 1).takeIf { it <= 200 },
        CatalogNumbers.neighbors(a),
      )
      assertEquals(a + 1, CatalogNumbers.displayPage(a))
      for (b in edges) {
        assertEquals(a xor b, CatalogNumbers.toggle(a, b))
        val error =
          if (b == 0) 0
          else if (a != 0) {
            if (a <= (b / 2).coerceAtMost(b - 20)) 0 else 2
          } else {
            if (b >= 10) 0 else 1
          }
        assertEquals(error, CatalogNumbers.pageRangeError(a, b))
      }
    }
    assertEquals(200, CatalogNumbers.toplistPages)
  }

  @Test
  fun buildersPreserveFieldTransitionsAndJvmParsing() {
    val builder = ListUrlBuilder()
    builder.setIndex("\ud800@2", false)
    builder.page = 2
    var state = builder.toGalleryListUrlState()
    assertEquals("\ud800@2", state.previous)
    assertEquals("1", state.seek)
    assertEquals(2, builder.page)
    builder.setSeek("2026-10-29")
    state = builder.toGalleryListUrlState()
    assertNull(state.previous)
    assertEquals("2", state.next)
    assertEquals("2026-10-29", state.seek)
    assertFailsWith<NumberFormatException> { builder.page }
    builder.setRange(-9)
    state = builder.toGalleryListUrlState()
    assertEquals(-9, state.range)
    assertNull(state.next)
    assertNull(state.seek)
    builder.page = Int.MIN_VALUE
    assertEquals(Int.MIN_VALUE, builder.page)
    assertEquals(-9, builder.toGalleryListUrlState().range)
    builder.page = 1
    assertNull(builder.toGalleryListUrlState().seek)
    for (name in listOf("", "abc", "@", "@1", "a@1@2", "x@\ud800", "\udc00@", "x@😀")) {
      assertEquals(name.substringAfterLast('@', "").ifEmpty { null }, savedSearchProgress(name))
    }
    val cleared =
      transitionCatalog(
        CatalogNavigation.from("p", "n", "s"),
        CatalogTransition.INDEX,
        null,
        forward = true,
      )
    assertNull(cleared.previous)
    assertNull(cleared.next)
    assertNull(cleared.seek)
  }

  @Test
  fun rejectsMalformedFramesBeforeBuilderStateCanChange() {
    val state = CatalogNavigation.from("p", "n", "s")
    for (response in
      listOf(
        byteArrayOf(0xff.toByte()),
        byteArrayOf(),
        rustCbor.encodeToByteArray(ApiEnvelope(2, "catalogState", state)),
        rustCbor.encodeToByteArray(ApiEnvelope(1, "wrong", state)),
        rustCbor.encodeToByteArray(
          ApiEnvelope(1, "catalogState", state.copy(next = byteArrayOf(1)))
        ),
      )) {
      assertFails { transitionCatalog(state, CatalogTransition.RANGE) { _, _ -> response } }
    }
    assertFailsWith<IllegalArgumentException> {
      transitionCatalog(state, CatalogTransition.INDEX, "x".repeat(8193)) { _, _ ->
        fail("oversized")
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      transitionCatalog(state, CatalogTransition.RANGE) { _, _ ->
        throw UnsatisfiedLinkError("fixture")
      }
    }
    val native =
      Class.forName("com.hippo.ehviewer.bridge.gallery.CatalogStateBridgeKt")
        .getDeclaredMethod(
          "nativeCatalogNavigationV1",
          ByteBuffer::class.java,
          Int::class.javaPrimitiveType,
        )
        .apply { isAccessible = true }
    for ((input, size) in
      listOf(
        ByteBuffer.allocate(8) to 8,
        ByteBuffer.allocateDirect(8) to -1,
        ByteBuffer.allocateDirect(8) to 9,
        ByteBuffer.allocateDirect(8) to 0,
      )) {
      assertFails { native.invoke(null, input, size) }
    }
  }
}
