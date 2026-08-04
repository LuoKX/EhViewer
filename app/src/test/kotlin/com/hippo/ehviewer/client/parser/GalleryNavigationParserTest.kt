package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.gallery.GalleryNavigationPlan
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationRequest
import com.hippo.ehviewer.bridge.gallery.GalleryNavigationTarget
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull

class GalleryNavigationParserTest {
  @Test
  fun disabledRolloutUsesOnlyTheFrozenDetailAndPageParsers() {
    var invoked = false
    val detail =
      GalleryDetailUrlParser.parseWithRustAdapter(
        DETAIL_URL,
        strict = true,
        rustEnabled = false,
      ) { _ ->
        invoked = true
        GalleryNavigationPlan.none()
      }
    val page =
      GalleryPageUrlParser.parseWithRustAdapter(PAGE_URL, strict = true, rustEnabled = false) { _ ->
        invoked = true
        GalleryNavigationPlan.none()
      }

    assertFalse(invoked)
    assertEquals(GalleryDetailUrlParser.Result(42, "0123456789"), detail)
    assertEquals(GalleryPageUrlParser.Result(42, "abcdef0123", 2), page)
  }

  @Test
  fun enabledBackendSendsTheExactGrammarInputWithoutLegacyPlanning() {
    val requests = mutableListOf<GalleryNavigationRequest>()
    val detail =
      GalleryDetailUrlParser.parseWithRustAdapter(
        DETAIL_URL,
        strict = true,
        rustEnabled = true,
      ) { request ->
        requests += request
        GalleryNavigationPlan.detail(42, "0123456789")
      }
    val page =
      GalleryPageUrlParser.parseWithRustAdapter(PAGE_URL, strict = false, rustEnabled = true) {
        request ->
        requests += request
        GalleryNavigationPlan.page(42, "abcdef0123", 2)
      }

    assertEquals(GalleryDetailUrlParser.Result(42, "0123456789"), detail)
    assertEquals(GalleryPageUrlParser.Result(42, "abcdef0123", 2), page)
    assertEquals(
      listOf(
        GalleryNavigationRequest(1, GalleryNavigationTarget.DETAIL, DETAIL_URL, strict = true),
        GalleryNavigationRequest(1, GalleryNavigationTarget.PAGE, PAGE_URL, strict = false),
      ),
      requests,
    )
  }

  @Test
  fun preservesLooseSearchPrefixAndZeroBasedPageBehavior() {
    assertEquals(
      GalleryDetailUrlParser.Result(42, "0123456789"),
      GalleryDetailUrlParser.parseWithRustAdapter(
        "prefix 42/0123456789! suffix",
        strict = false,
        rustEnabled = false,
      ) { _ ->
        GalleryNavigationPlan.none()
      },
    )
    assertEquals(
      GalleryPageUrlParser.Result(42, "abcdef0123", -1),
      GalleryPageUrlParser.parseWithRustAdapter(
        "prefix abcdef0123/42-0 suffix",
        strict = false,
        rustEnabled = false,
      ) { _ ->
        GalleryNavigationPlan.none()
      },
    )
    assertEquals(
      GalleryDetailUrlParser.Result(42, "0123456789"),
      GalleryDetailUrlParser.parseWithRustAdapter(
        "prefix $DETAIL_URL suffix",
        strict = true,
        rustEnabled = false,
      ) { _ ->
        GalleryNavigationPlan.none()
      },
    )
  }

  @Test
  fun noMatchAndNumericOverflowReturnNullWithoutInvokingRust() {
    var invocationCount = 0
    val callback: (GalleryNavigationRequest) -> GalleryNavigationPlan = { _ ->
      invocationCount++
      GalleryNavigationPlan.none()
    }

    assertNull(
      GalleryDetailUrlParser.parseWithRustAdapter(
        "https://example.com/g/42/0123456789",
        strict = true,
        rustEnabled = true,
        rustParser = callback,
      )
    )
    assertNull(
      GalleryDetailUrlParser.parseWithRustAdapter(
        "https://e-hentai.org/g/9223372036854775808/0123456789",
        strict = true,
        rustEnabled = true,
        rustParser = callback,
      )
    )
    assertNull(
      GalleryPageUrlParser.parseWithRustAdapter(
        "https://e-hentai.org/s/abcdef0123/42-2147483648",
        strict = true,
        rustEnabled = true,
        rustParser = callback,
      )
    )
    assertEquals(3, invocationCount)
  }

  @Test
  fun selectedRustFailuresPropagateAndReturnedPlanIsAdopted() {
    assertFailsWith<IllegalStateException> {
      GalleryDetailUrlParser.parseWithRustAdapter(
        DETAIL_URL,
        strict = true,
        rustEnabled = true,
      ) {
        throw IllegalStateException("transport failure")
      }
    }
    assertFailsWith<UnsatisfiedLinkError> {
      GalleryDetailUrlParser.parseWithRustAdapter(
        DETAIL_URL,
        strict = true,
        rustEnabled = true,
      ) {
        throw UnsatisfiedLinkError("missing native symbol")
      }
    }

    assertEquals(
      GalleryDetailUrlParser.Result(43, "0123456789"),
      GalleryDetailUrlParser.parseWithRustAdapter(DETAIL_URL, true, true) {
        GalleryNavigationPlan.detail(43, "0123456789")
      },
    )
  }

  private companion object {
    const val DETAIL_URL = "https://e-hentai.org/g/42/0123456789"
    const val PAGE_URL = "https://e-hentai.org/s/abcdef0123/42-3"
  }
}
