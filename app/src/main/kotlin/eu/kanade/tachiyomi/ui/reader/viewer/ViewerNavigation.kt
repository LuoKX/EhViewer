package eu.kanade.tachiyomi.ui.reader.viewer

import androidx.annotation.StringRes
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import com.ehviewer.core.i18n.R
import com.hippo.ehviewer.bridge.reader.readerFact
import com.hippo.ehviewer.bridge.reader.readerNavigation
import eu.kanade.tachiyomi.ui.reader.setting.TappingInvertMode

class ViewerNavigation private constructor(private val value: Int) {
  sealed class NavigationRegion(val code: Int, @StringRes val nameRes: Int, val colorRes: Int) {
    data object MENU :
      NavigationRegion(0, R.string.action_menu, com.hippo.ehviewer.R.color.navigation_menu)

    data object PREV :
      NavigationRegion(1, R.string.nav_zone_prev, com.hippo.ehviewer.R.color.navigation_prev)

    data object NEXT :
      NavigationRegion(2, R.string.nav_zone_next, com.hippo.ehviewer.R.color.navigation_next)

    data object LEFT :
      NavigationRegion(3, R.string.nav_zone_left, com.hippo.ehviewer.R.color.navigation_left)

    data object RIGHT :
      NavigationRegion(4, R.string.nav_zone_right, com.hippo.ehviewer.R.color.navigation_right)
  }

  data class Region(val rect: Rect, val type: NavigationRegion)

  fun regions(invertMode: TappingInvertMode = TappingInvertMode.NONE): NavigationRegions {
    val result =
      readerNavigation(
        2,
        intArrayOf(
          value,
          0,
          invertMode.shouldInvertHorizontal.readerFact(),
          invertMode.shouldInvertVertical.readerFact(),
        ),
      )
    return result.toList().chunked(5).map { (kind, left, top, right, bottom) ->
      Region(
        Rect(
          Float.fromBits(left),
          Float.fromBits(top),
          Float.fromBits(right),
          Float.fromBits(bottom),
        ),
        TYPES[kind],
      )
    }
  }

  companion object {
    private val TYPES =
      listOf(
        NavigationRegion.MENU,
        NavigationRegion.PREV,
        NavigationRegion.NEXT,
        NavigationRegion.LEFT,
        NavigationRegion.RIGHT,
      )

    private val MODES = (1..5).map { ViewerNavigation(it) }

    fun fromPreference(value: Int, isVertical: Boolean): ViewerNavigation {
      val mode = readerNavigation(10, intArrayOf(value, isVertical.readerFact()))[0]
      return MODES[mode - 1]
    }
  }
}

typealias NavigationRegions = List<ViewerNavigation.Region>

internal fun NavigationRegions.tapIntent(offset: Offset, rtl: Boolean, webtoon: Boolean): IntArray {
  val facts = IntArray(4 + size * 5)
  facts[0] = offset.x.toRawBits()
  facts[1] = offset.y.toRawBits()
  facts[2] = rtl.readerFact()
  facts[3] = webtoon.readerFact()
  forEachIndexed { index, region ->
    val offset = 4 + index * 5
    facts[offset] = region.type.code
    facts[offset + 1] = region.rect.left.toRawBits()
    facts[offset + 2] = region.rect.top.toRawBits()
    facts[offset + 3] = region.rect.right.toRawBits()
    facts[offset + 4] = region.rect.bottom.toRawBits()
  }
  return readerNavigation(3, facts)
}
