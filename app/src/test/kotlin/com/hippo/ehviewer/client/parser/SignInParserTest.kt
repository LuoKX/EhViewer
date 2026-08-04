package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.client.exception.EhException
import com.hippo.ehviewer.client.exception.ParseException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class SignInParserTest {
  @Test
  fun legacyParserPreservesSuccessAndBothErrorShapes() {
    assertEquals("Reader", SignInParser.parse("<p>You are now logged in as: Reader</p>"))
    assertEquals(
      "Wrong password",
      assertFailsWith<EhException> {
          SignInParser.parse("<h4>The error returned was:</h4><p>Wrong password</p>")
        }
        .message,
    )
    assertEquals(
      "Account locked",
      assertFailsWith<EhException> {
          SignInParser.parse("<span class=\"postcolor\">Account locked</span>")
        }
        .message,
    )
  }

  @Test
  fun legacyParserRejectsUnknownResponse() {
    assertFailsWith<ParseException> { SignInParser.parse("<html></html>") }
  }
}
