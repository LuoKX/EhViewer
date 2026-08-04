package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.SerializationException

class VoteCommentParserTest {
  @Test
  fun legacyParserPreservesUnknownFieldsAndSignedValues() {
    val result =
      VoteCommentParser.parse(
        """{"comment_id":-1,"comment_score":-7,"comment_vote":0,"error":"ignored","extra":true}"""
      )

    assertEquals(VoteCommentResult(id = -1, score = -7, vote = 0), result)
  }

  @Test
  fun legacyParserAcceptsNumericStrings() {
    val result =
      VoteCommentParser.parse("""{"comment_id":"1","comment_score":"1","comment_vote":"1"}""")

    assertEquals(VoteCommentResult(id = 1, score = 1, vote = 1), result)
  }

  @Test
  fun legacyParserRejectsMissingNullOverflowAndMalformedFields() {
    listOf(
        """{"comment_score":1,"comment_vote":1}""",
        """{"comment_id":null,"comment_score":1,"comment_vote":1}""",
        """{"comment_id":1,"comment_score":2147483648,"comment_vote":1}""",
        """{"comment_id":1,"comment_score":1,"comment_vote":null}""",
        """{"error":"service failure"}""",
        """{"comment_id":1,"comment_score":1,"comment_vote":}""",
      )
      .forEach { body ->
        assertFailsWith<SerializationException>("Expected parser to reject: $body") {
          VoteCommentParser.parse(body)
        }
      }
  }

  @Test
  fun legacyParserUsesTheLastDuplicateField() {
    val result =
      VoteCommentParser.parse(
        """{"comment_id":1,"comment_id":9,"comment_score":2,"comment_score":8,"comment_vote":-1,"comment_vote":7}"""
      )

    assertEquals(VoteCommentResult(id = 9, score = 8, vote = 7), result)
  }

  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() = runBlocking {
    var legacyCalls = 0
    var rustCalls = 0
    val result =
      VoteCommentParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
          legacyCalls += 1
          VoteCommentResult(id = 1, score = 2, vote = 1)
        },
        rustParser = {
          rustCalls += 1
          error("V1 parser must remain disabled")
        },
      )

    assertEquals(VoteCommentResult(id = 1, score = 2, vote = 1), result)
    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)
  }

  @Test
  fun enabledPathInvokesOnlyV1AndNeverReplaysAfterFailureOrCancellation() = runBlocking {
    var legacyCalls = 0
    val success =
      VoteCommentParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          VoteCommentResult(id = 1, score = 2, vote = 1)
        },
        rustParser = { VoteCommentResult(id = 9, score = 8, vote = 7) },
      )
    assertEquals(VoteCommentResult(id = 9, score = 8, vote = 7), success)
    assertEquals(0, legacyCalls)

    val failure = RustError(RustErrorKind.ERROR, "Gallery comment vote response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        VoteCommentParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalls += 1
            VoteCommentResult(id = 1, score = 2, vote = 1)
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertEquals(0, legacyCalls)

    assertFailsWith<CancellationException> {
      VoteCommentParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          VoteCommentResult(id = 1, score = 2, vote = 1)
        },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertEquals(0, legacyCalls)
  }
}
