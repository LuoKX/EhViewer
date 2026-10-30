package com.hippo.ehviewer.ui.reader

import androidx.compose.ui.geometry.Offset
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.reader.readerFact
import com.hippo.ehviewer.bridge.reader.readerNavigation
import me.saket.telephoto.zoomable.DoubleClickToZoomListener
import me.saket.telephoto.zoomable.ZoomableState

object DoubleTapZoom : DoubleClickToZoomListener {
  override suspend fun onDoubleClick(state: ZoomableState, centroid: Offset) {
    val enabled = Settings.doubleTapToZoom.value
    val fraction = if (enabled) state.zoomFraction else null
    when (
      readerNavigation(
        8,
        intArrayOf(
          enabled.readerFact(),
          (fraction != null).readerFact(),
          (fraction ?: 0f).toRawBits(),
        ),
      )[0]
    ) {
      1 -> state.resetZoom()
      2 ->
        state.zoomTo(
          zoomFactor = state.contentTransformation.scaleMetadata.initialScale.scaleX * 2f,
          centroid = centroid,
        )
    }
  }
}
