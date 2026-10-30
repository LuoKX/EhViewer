package com.hippo.ehviewer.ui.reader

import androidx.compose.ui.Modifier
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import com.hippo.ehviewer.bridge.reader.readerFact
import com.hippo.ehviewer.bridge.reader.readerNavigation

fun Modifier.keyEventHandler(
  volumeKeysEnabled: () -> Boolean,
  volumeKeysInverted: () -> Boolean,
  movePrevious: () -> Unit,
  moveNext: () -> Unit,
) = onPreviewKeyEvent {
  val down = it.type == KeyEventType.KeyDown
  val key =
    if (down)
      when (it.key) {
        Key.DirectionUp,
        Key.PageUp -> 1
        Key.DirectionDown,
        Key.PageDown -> 2
        Key.VolumeUp -> 3
        Key.VolumeDown -> 4
        else -> 0
      }
    else 0
  val volume = key in 3..4 && volumeKeysEnabled()
  val inverted = volume && volumeKeysInverted()
  when (
    readerNavigation(
      5,
      intArrayOf(key, down.readerFact(), volume.readerFact(), inverted.readerFact()),
    )[0]
  ) {
    1 -> movePrevious()
    2 -> moveNext()
    else -> return@onPreviewKeyEvent false
  }
  true
}
