package com.hippo.ehviewer.ui.tools

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.foundation.text.contextmenu.builder.item
import androidx.compose.foundation.text.contextmenu.data.TextContextMenuKeys
import androidx.compose.foundation.text.contextmenu.modifier.appendTextContextMenuComponents
import androidx.compose.foundation.text.contextmenu.modifier.filterTextContextMenuComponents
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.getSelectedText
import androidx.compose.ui.text.input.getTextAfterSelection
import androidx.compose.ui.text.input.getTextBeforeSelection
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import com.ehviewer.core.i18n.R
import com.ehviewer.core.util.unreachable
import com.hippo.ehviewer.bridge.text.TextSpan
import com.hippo.ehviewer.bridge.text.TextSpansRequest
import com.hippo.ehviewer.bridge.text.planTextSpans
import io.github.petertrr.diffutils.diffInline
import io.github.petertrr.diffutils.patch.DeleteDelta
import io.github.petertrr.diffutils.patch.InsertDelta
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import moe.tarsin.string

fun TextFieldValue.updateSpan(origin: TextFieldValue): TextFieldValue {
  val oriSpan = origin.annotatedString.spanStyles
  val annoStr = origin.annotatedString
  val len = origin.text.length
  if (oriSpan.isEmpty()) {
    // User have no spanned comment, just update
    return this
  }
  // Hacky: BasicTextField would clear text spans
  val diff = diffInline(origin.text, text).deltas
  if (diff.isNotEmpty()) {
    if (diff.size == 1) {
      val str =
        when (val delta = diff.first()) {
          is DeleteDelta -> {
            val pos = delta.source.position
            val toIns = delta.source.lines.first()
            val end = pos + toIns.length // Exclusive
            buildAnnotatedString {
              append(annoStr.subSequence(0, pos))
              append(annoStr.subSequence(end, len))
            }
          }
          is InsertDelta -> {
            val pos = delta.source.position
            val toIns = delta.target.lines.first()
            buildAnnotatedString {
              append(annoStr.subSequence(0, pos))
              append(toIns)
              append(annoStr.subSequence(pos, len))
            }
          }
          else -> unreachable()
        }
      // Update span range for updated comment
      return copy(annotatedString = str)
    } else {
      // Cannot handle diff, override directly
      return this
    }
  } else {
    // Raw Text not changed, copy annotatedString
    return copy(annotatedString = origin.annotatedString)
  }
}

fun AnnotatedString.normalizeSpan(): AnnotatedString {
  if (spanStyles.isEmpty()) return this
  val observations = spanStyles.map {
    val style = it.item
    val flags =
      (if (style.fontWeight == FontWeight.Bold) 1 else 0) or
        (if (style.fontStyle == FontStyle.Italic) 2 else 0) or
        (if (style.textDecoration == TextDecoration.Underline) 4 else 0) or
        (if (style.textDecoration == TextDecoration.LineThrough) 8 else 0)
    TextSpan(it.start, it.end, flags)
  }
  val spans =
    planTextSpans(TextSpansRequest(textLength = text.length, spans = observations)).map {
      val style =
        when (it.flags) {
          1 -> SpanStyle(fontWeight = FontWeight.Bold)
          2 -> SpanStyle(fontStyle = FontStyle.Italic)
          4 -> SpanStyle(textDecoration = TextDecoration.Underline)
          8 -> SpanStyle(textDecoration = TextDecoration.LineThrough)
          else -> error("Invalid Text Spans style")
        }
      AnnotatedString.Range(style, it.start, it.end)
    }
  return AnnotatedString(text, spans)
}

// Overlapped SpanStyle is support through stack based builder
// Does BBCode allow interlaced tags? If not, split interlaced span first?
fun AnnotatedString.toBBCode() = buildString {
  val len = text.length
  fun SpanStyle.push() =
    when {
      fontWeight == FontWeight.Bold -> append("[b]")
      fontStyle == FontStyle.Italic -> append("[i]")
      textDecoration == TextDecoration.Underline -> append("[u]")
      textDecoration == TextDecoration.LineThrough -> append("[s]")
      else -> this@buildString
    }
  fun SpanStyle.pop() =
    when {
      fontWeight == FontWeight.Bold -> append("[/b]")
      fontStyle == FontStyle.Italic -> append("[/i]")
      textDecoration == TextDecoration.Underline -> append("[/u]")
      textDecoration == TextDecoration.LineThrough -> append("[/s]")
      else -> this@buildString
    }
  var current = 0
  val stack = ArrayDeque<AnnotatedString.Range<SpanStyle>>()
  val spans = spanStyles.groupBy { it.start }
  while (true) {
    while (stack.lastOrNull()?.end == current) {
      stack.removeLast().item.pop()
    }
    if (current == len) break
    spans[current]
      ?.sortedByDescending { it.end }
      ?.forEach {
        it.item.push()
        stack.addLast(it)
      }
    append(text[current])
    current++
  }
}

@Composable
context(_: Context)
fun Modifier.addBBCodeTextContextMenuItems(textFieldValue: MutableState<TextFieldValue>): Modifier {
  var tfv by textFieldValue
  val coroutineScope = rememberCoroutineScope()
  return appendTextContextMenuComponents {
    if (tfv.selection.collapsed) return@appendTextContextMenuComponents
    BBCodeFormat.entries.forEach { item ->
      item(key = item, label = string(item.id)) {
        val capturedTfv = tfv
        val start = capturedTfv.selection.min
        val end = capturedTfv.selection.max
        val annotatedString = buildAnnotatedString {
          val len = capturedTfv.text.length
          fun addStyle(style: SpanStyle) {
            append(capturedTfv.annotatedString)
            addStyle(style, start, end)
          }
          fun addTextDecoration(decoration: TextDecoration) {
            append(capturedTfv.getTextBeforeSelection(len))
            val ans = capturedTfv.getSelectedText()
            val spans =
              ans.spanStyles.filter {
                it.item.textDecoration == null || it.item.textDecoration == decoration
              }
            withStyle(SpanStyle(textDecoration = decoration)) {
              append(AnnotatedString(ans.text, spans))
            }
            append(capturedTfv.getTextAfterSelection(len))
          }
          when (item) {
            BBCodeFormat.Bold -> addStyle(SpanStyle(fontWeight = FontWeight.Bold))
            BBCodeFormat.Italic -> addStyle(SpanStyle(fontStyle = FontStyle.Italic))
            BBCodeFormat.Underline -> addTextDecoration(TextDecoration.Underline)
            BBCodeFormat.Strikethrough -> addTextDecoration(TextDecoration.LineThrough)
            BBCodeFormat.Url -> append(capturedTfv.annotatedString)
            BBCodeFormat.Clear -> {
              append(capturedTfv.getTextBeforeSelection(len))
              append(capturedTfv.getSelectedText().text)
              append(capturedTfv.getTextAfterSelection(len))
            }
          }
        }
        tfv =
          TextFieldValue(
            annotatedString =
              buildAnnotatedString {
                append(annotatedString)
                // Hacky: Trigger recomposition to hide text toolbar
                append(Char.MIN_VALUE)
              },
            selection = TextRange(end),
          )
        coroutineScope.launch {
          // Hacky: Let TextField recompose first
          delay(100)
          tfv = tfv.copy(annotatedString = annotatedString)
        }
        close()
      }
    }
  }
    .filterTextContextMenuComponents {
      when (it.key) {
        TextContextMenuKeys.CutKey,
        TextContextMenuKeys.CopyKey,
        TextContextMenuKeys.PasteKey,
        TextContextMenuKeys.SelectAllKey,
        is BBCodeFormat -> true
        else -> false
      }
    }
}

private enum class BBCodeFormat(@StringRes val id: Int) {
  Bold(R.string.format_bold),
  Italic(R.string.format_italic),
  Underline(R.string.format_underline),
  Strikethrough(R.string.format_strikethrough),
  Url(R.string.format_url),
  Clear(R.string.format_plain),
}
