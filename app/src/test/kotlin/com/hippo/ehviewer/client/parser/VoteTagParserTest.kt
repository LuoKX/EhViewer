package com.hippo.ehviewer.client.parser

import com.ehviewer.core.model.GalleryTagGroup
import com.ehviewer.core.model.TagNamespace
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking

class VoteTagParserTest {
  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() = runBlocking {
    var legacyCalls = 0
    var rustCalls = 0
    val result =
      VoteTagParser.parseWithAdapters(
        useRust = false,
        legacyParser = {
          legacyCalls += 1
          listOf(GalleryTagGroup(TagNamespace.Artist, emptyList()))
        },
        rustParser = {
          rustCalls += 1
          error("V1 parser must remain disabled")
        },
      )

    assertEquals(TagNamespace.Artist, result.single().namespace)
    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)
  }

  @Test
  fun enabledPathInvokesOnlyV1AndNeverReplaysAfterFailureOrCancellation() = runBlocking {
    var legacyCalls = 0
    val success =
      VoteTagParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          listOf(GalleryTagGroup(TagNamespace.Artist, emptyList()))
        },
        rustParser = { listOf(GalleryTagGroup(TagNamespace.Language, emptyList())) },
      )
    assertEquals(TagNamespace.Language, success.single().namespace)
    assertEquals(0, legacyCalls)

    val failure = RustError(RustErrorKind.ERROR, "Gallery tag vote response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        VoteTagParser.parseWithAdapters(
          useRust = true,
          legacyParser = {
            legacyCalls += 1
            emptyList()
          },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertEquals(0, legacyCalls)

    assertFailsWith<CancellationException> {
      VoteTagParser.parseWithAdapters(
        useRust = true,
        legacyParser = {
          legacyCalls += 1
          emptyList()
        },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertEquals(0, legacyCalls)
  }
}
