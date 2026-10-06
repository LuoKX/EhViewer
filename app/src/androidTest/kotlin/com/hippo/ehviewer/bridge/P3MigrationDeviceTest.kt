package com.hippo.ehviewer.bridge

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.files.FileRuleKind
import com.hippo.ehviewer.bridge.files.planFileRules
import com.hippo.ehviewer.bridge.text.TextSpan
import com.hippo.ehviewer.bridge.text.TextSpansRequest
import com.hippo.ehviewer.bridge.text.planTextSpans
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test
import org.junit.runner.RunWith

/** Offline JNI probes; compilation alone is not device-execution evidence. */
@RunWith(AndroidJUnit4::class)
class P3MigrationDeviceTest {
  @Test
  fun filenameRulesUseRealJniAndPreserveOldSemantics() {
    assertEquals("a__中😀", planFileRules(FileRuleKind.SANITIZE, "a:/中😀. "))
    assertEquals(200, planFileRules(FileRuleKind.SANITIZE, "a".repeat(201))!!.length)
    assertEquals("\uD800", planFileRules(FileRuleKind.BASENAME, "\uD800"))
    assertNull(planFileRules(FileRuleKind.BASENAME, ".jpg"))
    assertEquals("JPG", planFileRules(FileRuleKind.EXTENSION, "a.JPG"))
    assertThrows(IllegalStateException::class.java) { planFileRules(FileRuleKind.SANITIZE, "... ") }
  }

  @Test
  fun allTextStylesNormalizeInOneRealJniBatch() {
    assertEquals(
      listOf(TextSpan(0, 6, 1), TextSpan(3, 6, 2)),
      planTextSpans(
        TextSpansRequest(textLength = 12, spans = listOf(TextSpan(3, 6, 3), TextSpan(0, 3, 1)))
      ),
    )
  }
}
