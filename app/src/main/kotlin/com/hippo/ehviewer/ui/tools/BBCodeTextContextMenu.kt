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
import com.hippo.ehviewer.bridge.text.BBCodeAction
import com.hippo.ehviewer.bridge.text.BBCodeOperation
import com.hippo.ehviewer.bridge.text.BBCodeRequest
import com.hippo.ehviewer.bridge.text.TextSpan
import com.hippo.ehviewer.bridge.text.TextSpansRequest
import com.hippo.ehviewer.bridge.text.planBBCode
import com.hippo.ehviewer.bridge.text.planTextSpans
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import moe.tarsin.string

fun TextFieldValue.updateSpan(origin: TextFieldValue): TextFieldValue {
  if (origin.annotatedString.spanStyles.isEmpty()) return this
  val plan =
    planBBCode(
      BBCodeRequest(
        operation = BBCodeOperation.EDIT,
        units = text.map { it.code },
        originUnits = origin.text.map { it.code },
        spans = emptyList(),
      )
    )
  return when (plan.action) {
    BBCodeAction.ORIGINAL -> copy(annotatedString = origin.annotatedString)
    BBCodeAction.TARGET -> this
    BBCodeAction.SPLICE -> {
      val edit = checkNotNull(plan.edit)
      // Rust owns slice positions; Compose preserves every opaque annotation on those slices.
      copy(
        annotatedString =
          buildAnnotatedString {
            append(origin.annotatedString.subSequence(0, edit.sourceStart))
            append(text.substring(edit.targetStart, edit.targetEnd))
            append(origin.annotatedString.subSequence(edit.sourceEnd, origin.text.length))
          }
      )
    }
    BBCodeAction.EXPORT -> error("Unexpected BBCode export action")
  }
}

private fun AnnotatedString.observeSpans(): List<TextSpan> = spanStyles.map {
  val style = it.item
  val flags =
    (if (style.fontWeight == FontWeight.Bold) 1 else 0) or
      (if (style.fontStyle == FontStyle.Italic) 2 else 0) or
      (if (style.textDecoration == TextDecoration.Underline) 4 else 0) or
      (if (style.textDecoration == TextDecoration.LineThrough) 8 else 0)
  TextSpan(it.start, it.end, flags)
}

fun AnnotatedString.normalizeSpan(): AnnotatedString {
  if (spanStyles.isEmpty()) return this
  val spans =
    planTextSpans(TextSpansRequest(textLength = text.length, spans = observeSpans())).map {
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

/** Tag order, stack traversal and UTF-16 serialization are Rust-owned. */
fun AnnotatedString.toBBCode(): String {
  val spans = observeSpans()
  val plan =
    planBBCode(
      BBCodeRequest(
        operation = BBCodeOperation.EXPORT,
        units = text.map { it.code },
        originUnits = null,
        spans = spans,
      )
    )
  val units = checkNotNull(plan.units)
  return CharArray(units.size) { units[it].toChar() }.concatToString()
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
