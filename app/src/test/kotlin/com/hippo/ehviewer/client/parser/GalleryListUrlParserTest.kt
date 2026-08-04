package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.gallery.GalleryListUrlParseRequest
import com.hippo.ehviewer.bridge.gallery.GalleryListUrlState
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull

class GalleryListUrlParserTest {
  @Test
  fun disabledRolloutReturnsLegacyWithoutInvokingRust() {
    var invoked = false

    val result =
      GalleryListUrlParser.parseWithRustAdapter(TAG_URL, rustEnabled = false) {
        invoked = true
        null
      }

    assertFalse(invoked)
    assertEquals(2, result?.mode)
    assertEquals("artist:alice", result?.keyword)
  }

  @Test
  fun enabledRolloutSendsOnlyTheClosedSanitizedSnapshot() {
    var captured: GalleryListUrlParseRequest? = null

    val result =
      GalleryListUrlParser.parseWithRustAdapter(
        "https://e-hentai.org/tag/artist%253Aalice/ignored-secret" +
          "?prev=first&prev=second&f_search=cats&unrelated=ignored",
        rustEnabled = true,
      ) { request ->
        captured = request
        tagState()
      }

    assertEquals(listOf("tag", "artist%3Aalice"), captured?.snapshot?.segments)
    assertEquals("first", captured?.snapshot?.parameters?.previous)
    assertEquals("cats", captured?.snapshot?.parameters?.search)
    assertEquals("artist:alice", result?.keyword)
  }

  @Test
  fun preservesEmptyImageHashPresenceAndRejectsUnsupportedUrlsBeforeRust() {
    var invocationCount = 0
    val imageSearch =
      GalleryListUrlParser.parseWithRustAdapter(
        "https://e-hentai.org/?f_shash=",
        rustEnabled = true,
      ) { request ->
        invocationCount++
        assertEquals("", request.snapshot.parameters.imageHash)
        tagState().copy(mode = 4, keyword = null, imageHash = "")
      }

    assertEquals(4, imageSearch?.mode)
    assertEquals("", imageSearch?.hash)
    assertNull(
      GalleryListUrlParser.parseWithRustAdapter(
        "https://example.com/tag/value",
        rustEnabled = true,
      ) {
        invocationCount++
        null
      }
    )
    assertNull(
      GalleryListUrlParser.parseWithRustAdapter(
        "https://e-hentai.org/tag",
        rustEnabled = true,
      ) {
        invocationCount++
        null
      }
    )
    assertEquals(2, invocationCount)
  }

  @Test
  fun selectedRustFailuresPropagateAndReturnedStateIsAdopted() {
    assertFailsWith<IllegalStateException> {
      GalleryListUrlParser.parseWithRustAdapter(TAG_URL, rustEnabled = true) {
        throw IllegalStateException("transport failure")
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      GalleryListUrlParser.parseWithRustAdapter(TAG_URL, rustEnabled = true) {
        throw UnsatisfiedLinkError("missing native symbol")
      }
    }
    val rustState = tagState().copy(mode = 0)
    val adopted = GalleryListUrlParser.parseWithRustAdapter(TAG_URL, true) { rustState }

    assertEquals(rustState, adopted?.toGalleryListUrlState())
  }

  private fun tagState() =
    GalleryListUrlState(
      mode = 2,
      previous = null,
      next = null,
      seek = null,
      range = 0,
      category = -1,
      keyword = "artist:alice",
      imageHash = null,
      language = -1,
      advancedSearch = -1,
      minimumRating = -1,
      pageFrom = -1,
      pageTo = -1,
    )

  private companion object {
    const val TAG_URL = "https://e-hentai.org/tag/artist%253Aalice"
  }
}
