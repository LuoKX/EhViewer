package com.hippo.ehviewer.bridge.reader

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import com.hippo.ehviewer.bridge.updater.UpdaterHostLibrary
import eu.kanade.tachiyomi.ui.reader.setting.TappingInvertMode
import eu.kanade.tachiyomi.ui.reader.viewer.ViewerNavigation
import eu.kanade.tachiyomi.ui.reader.viewer.tapIntent
import java.io.IOException
import kotlin.test.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class ReaderNavigationBridgeTest {
  init {
    UpdaterHostLibrary.load()
  }

  @Test
  fun sourceSelectionReadsProgressBeforeArchiveAndKeepsExplicitPages() = runBlocking {
    for (page in listOf(-1, 0, 7)) for (archive in listOf(null, "fixture.cbz")) {
      val events = mutableListOf<String>()
      val result =
        readerGallerySource(
          page,
          {
            events += "progress"
            3
          },
          {
            events += "archive"
            archive
          },
        )
      assertEquals(if (page == -1) 3 else page, result.page)
      assertEquals(archive, result.archive)
      assertEquals(if (page == -1) listOf("progress", "archive") else listOf("archive"), events)
    }
    val failure = IOException("progress")
    assertSame(
      failure,
      assertFails {
        readerGallerySource(-1, { throw failure }, { fail("archive read after failed progress") })
      },
    )
    for (info in listOf(false, true)) for (name in
      listOf("a.cbz", "a.zip", "a.ZIP", "a.zip ", "\uD800.zip")) {
      assertEquals(!info || name.endsWith(".zip"), readerArchiveSorting(info, name))
    }
  }

  @Test
  fun modeSuffixAndUnknownPreferencesMatchTheOriginalRules() {
    for (tags in
      listOf(
        null,
        emptyList(),
        listOf("other:webtoon"),
        listOf("notwebtoon"),
        listOf("WEBTOON"),
        listOf("webtoon "),
        listOf("\uD800webtoon"),
      )) {
      for (pref in listOf(Int.MIN_VALUE, -1, 0, 1, 2, 3, 4, 5, 6, Int.MAX_VALUE)) {
        val expected =
          if (pref in 1..5) pref else if (tags?.any { it.endsWith("webtoon") } == true) 4 else 2
        assertContentEquals(
          intArrayOf(
            expected,
            (expected >= 3).readerFact(),
            (expected >= 4).readerFact(),
            (expected == 2).readerFact(),
          ),
          readerMode(pref, tags),
        )
      }
    }
    for (rtl in 0..1) for (layout in 0..1) for (vertical in 0..1) assertContentEquals(
      intArrayOf(
        (vertical == 0 && rtl == 1).readerFact(),
        (vertical == 0 && rtl != layout).readerFact(),
      ),
      readerNavigation(9, intArrayOf(rtl, layout, vertical)),
    )
  }

  private fun originalRegions(value: Int, vertical: Boolean): List<Pair<Int, Rect>> =
    when (if (value in 1..5) value else if (vertical) 1 else 4) {
      1 ->
        listOf(
          1 to Rect(0f, .33f, .33f, .66f),
          1 to Rect(0f, 0f, 1f, .33f),
          2 to Rect(.66f, .33f, 1f, .66f),
          2 to Rect(0f, .66f, 1f, 1f),
        )
      2 -> listOf(2 to Rect(.33f, .33f, 1f, 1f), 1 to Rect(0f, .33f, .33f, 1f))
      3 ->
        listOf(
          2 to Rect(0f, 0f, .33f, 1f),
          1 to Rect(.33f, .66f, .66f, 1f),
          2 to Rect(.66f, 0f, 1f, 1f),
        )
      4 -> listOf(3 to Rect(0f, 0f, .33f, 1f), 4 to Rect(.66f, 0f, 1f, 1f))
      else -> emptyList()
    }

  @Test
  fun regionRenderingAndTapDecisionsMatchLegacyEdgesAllModesAndInversions() {
    assertSame(ViewerNavigation.fromPreference(1, false), ViewerNavigation.fromPreference(0, true))
    assertSame(ViewerNavigation.fromPreference(4, true), ViewerNavigation.fromPreference(-1, false))
    assertSame(ViewerNavigation.fromPreference(3, true), ViewerNavigation.fromPreference(3, false))
    val coordinates =
      listOf(
        -1f,
        -0f,
        0f,
        Math.nextDown(.33f),
        .33f,
        Math.nextUp(.33f),
        .5f,
        .66f,
        Math.nextUp(.66f),
        1f,
        Float.NaN,
        Float.POSITIVE_INFINITY,
      )
    for (pref in -1..6) for (vertical in listOf(false, true)) for (invert in
      TappingInvertMode.entries) {
      val old =
        originalRegions(pref, vertical).map { (type, rect) ->
          type to
            Rect(
              if (invert.shouldInvertHorizontal) 1f - rect.right else rect.left,
              if (invert.shouldInvertVertical) 1f - rect.bottom else rect.top,
              if (invert.shouldInvertHorizontal) 1f - rect.left else rect.right,
              if (invert.shouldInvertVertical) 1f - rect.top else rect.bottom,
            )
        }
      val regions = ViewerNavigation.fromPreference(pref, vertical).regions(invert)
      assertEquals(old, regions.map { it.type.code to it.rect })
      for (x in coordinates) for (y in coordinates) for (rtl in listOf(false, true)) for (webtoon in
        listOf(false, true)) {
        val point = Offset(x, y)
        val hit = old.find { point in it.second }?.first ?: 0
        val expected =
          when {
            hit == 0 -> intArrayOf(0, 0)
            webtoon -> intArrayOf(if (hit == 2 || hit == 4) 2 else 1, 0)
            hit == 1 -> intArrayOf(1, if (rtl) 1 else 2)
            hit == 2 -> intArrayOf(2, if (rtl) 2 else 1)
            hit == 3 -> intArrayOf(if (rtl) 2 else 1, 2)
            else -> intArrayOf(if (rtl) 1 else 2, 1)
          }
        assertContentEquals(expected, regions.tapIntent(point, rtl, webtoon))
      }
    }
  }

  @Test
  fun panObservationsAreLazyAndErrorsOrCancellationNeverFallThroughToTurnPage() = runBlocking {
    for (side in 0..2) for (available in listOf(false, true)) for (remaining in
      listOf(1f, Math.nextUp(1f), Float.NaN, Float.POSITIVE_INFINITY)) {
      val events = mutableListOf<String>()
      executeReaderTap(
        intArrayOf(2, side),
        {
          events += "available"
          available
        },
        {
          events += "bounds:$it"
          remaining
        },
        { events += "pan:$it" },
        { events += "page:$it" },
        { fail("menu") },
      )
      val expected = buildList {
        if (side != 0) {
          add("available")
          if (available) add("bounds:$side")
        }
        if (side != 0 && available && remaining > 1f) add("pan:$side") else add("page:2")
      }
      assertEquals(expected, events)
    }
    executeReaderTap(
      intArrayOf(0, 0),
      { fail("menu availability") },
      { fail("menu bounds") },
      { fail("menu pan") },
      { fail("menu page") },
      {},
    )
    for (failure in listOf(IOException("pan"), CancellationException("cancel"))) {
      assertSame(
        failure,
        assertFails {
          executeReaderTap(
            intArrayOf(2, 1),
            { true },
            { 2f },
            { throw failure },
            { fail("page after failed pan") },
            { fail("menu") },
          )
        },
      )
    }
  }

  @Test
  fun keysPageEdgesAndDoubleTapKeepDiscreteThresholds() {
    for (key in 0..4) for (down in 0..1) for (volume in 0..1) for (inverted in 0..1) {
      val expected =
        when {
          down == 0 -> 0
          key < 3 -> key
          volume == 0 -> 0
          (key == 3) == (inverted == 0) -> 1
          else -> 2
        }
      assertEquals(expected, readerNavigation(5, intArrayOf(key, down, volume, inverted))[0])
    }
    for (page in listOf(Int.MIN_VALUE, -1, 0, 1, 9, Int.MAX_VALUE)) for (count in
      listOf(0, 1, 10, Int.MAX_VALUE)) for (previous in listOf(false, true)) {
      val target = if (previous) page - 1 else page + 1
      assertEquals(
        target.takeIf { if (previous) it >= 0 else it < count },
        readerPageTarget(page, count, previous),
      )
    }
    for (enabled in 0..1) for (ready in 0..1) for (fraction in
      listOf(Float.NaN, .05f, Math.nextUp(.05f), Float.POSITIVE_INFINITY)) {
      assertEquals(
        if (enabled == 0 || ready == 0) 0 else if (fraction > .05f) 1 else 2,
        readerNavigation(8, intArrayOf(enabled, ready, fraction.toRawBits()))[0],
      )
    }
  }

  @Test
  fun badFramesAndPlansFailWithoutEffects() {
    assertFails { readerNavigation(2, intArrayOf(1, 2, 0, 0)) }
    assertFails { readerNavigation(1, intArrayOf(0, 1)) }
    assertFails { readerNavigation(1, intArrayOf(0, 1, 65536)) }
    assertFails { readerNavigation(99, intArrayOf()) }
    for ((op, bad) in
      listOf(
        0 to intArrayOf(1, 3),
        1 to intArrayOf(0, 0, 0, 0),
        2 to intArrayOf(1),
        3 to intArrayOf(0, 9),
        4 to intArrayOf(2),
        5 to intArrayOf(),
        6 to intArrayOf(9, 0),
        7 to intArrayOf(2),
        8 to intArrayOf(3),
        9 to intArrayOf(1, 2),
      )) {
      assertFails { readerNavigation(op, intArrayOf()) { _, _ -> bad } }
    }
  }
}
