package com.hippo.ehviewer.bridge

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.withStyle
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.ui.tools.normalizeSpan
import com.hippo.ehviewer.ui.tools.toBBCode
import com.hippo.ehviewer.ui.tools.updateSpan
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real offline JNI probes; compilation is not device execution evidence. */
@RunWith(AndroidJUnit4::class)
class P4MigrationDeviceTest {
  private fun original() =
    TextFieldValue(
      buildAnnotatedString {
        pushStringAnnotation("opaque", "kept")
        withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append("abcd") }
        pop()
      }
    )

  @Test
  fun exportAndNormalizationUseRealJni() {
    assertEquals("[b]abcd[/b]", original().annotatedString.normalizeSpan().toBBCode())
    val raw =
      AnnotatedString(
        "\uD800a",
        listOf(AnnotatedString.Range(SpanStyle(fontWeight = FontWeight.Bold), 0, 1)),
      )
    assertEquals("[b]\uD800[/b]a", raw.toBBCode())
  }

  @Test
  fun insertionPreservesOpaqueAnnotationsSelectionAndComposition() {
    val incoming = TextFieldValue("abXcd", selection = TextRange(3), composition = TextRange(2, 3))
    val result = incoming.updateSpan(original())
    assertEquals(incoming.text, result.text)
    assertEquals(incoming.selection, result.selection)
    assertEquals(incoming.composition, result.composition)
    assertEquals(
      listOf(0 to 2, 3 to 5),
      result.annotatedString.spanStyles.map { it.start to it.end },
    )
    assertEquals(
      listOf(0 to 2, 3 to 5),
      result.annotatedString.getStringAnnotations("opaque", 0, 5).map { it.start to it.end },
    )
  }

  @Test
  fun deletionAndUnchangedTextKeepOriginalSliceSemantics() {
    val origin = original()
    val deleted = TextFieldValue("ad").updateSpan(origin)
    assertEquals("ad", deleted.text)
    assertEquals(
      listOf(0 to 1, 1 to 2),
      deleted.annotatedString.spanStyles.map { it.start to it.end },
    )
    assertEquals(origin.annotatedString, TextFieldValue("abcd").updateSpan(origin).annotatedString)
    val plain = TextFieldValue("plain")
    assertSame(plain, plain.updateSpan(TextFieldValue("origin")))
  }

  @Test
  fun replacementAndMultiHunkEditsKeepIncomingTextWithoutOldCrash() {
    for (text in listOf("abXd", "XabYcd")) {
      val incoming = TextFieldValue(text, selection = TextRange(text.length))
      assertSame(incoming, incoming.updateSpan(original()))
    }
  }
}
