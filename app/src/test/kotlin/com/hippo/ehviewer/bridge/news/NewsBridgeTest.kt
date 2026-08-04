package com.hippo.ehviewer.bridge.news

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.NewsPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class NewsBridgeTest {
  @Test
  fun decodesAndMapsTheRustGeneratedGoldenFixture() {
    val envelope = Cbor.decodeFromByteArray<ApiEnvelope<NewsPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("news", envelope.module)
    assertEquals("<strong>Event &amp; reward</strong>", envelope.toEventHtml())
  }

  @Test
  fun preservesAbsentAndEmptyEventPaneValues() {
    for (eventHtml in listOf(null, "")) {
      val envelope = ApiEnvelope(version = 1, module = "news", payload = NewsPage(eventHtml))
      assertEquals(eventHtml, envelope.toEventHtml())
    }
  }

  @Test
  fun rejectsUnsupportedContractsAndPreservesTypedFailures() {
    val payload = NewsPage("<strong>Event</strong>")
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "news", payload = payload).toEventHtml()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "eventPane", payload = payload).toEventHtml()
    }

    val failure = RustError(RustErrorKind.IP_BANNED, "news response is not HTML")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<NewsPage>(version = 1, module = "news", error = failure).toEventHtml()
      }
    assertEquals(failure, error.failure)
  }

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c65646e657773677061796c6f6164a1696576656e744874" +
        "6d6c78233c7374726f6e673e4576656e742026616d703b207265776172643c2f7374726f6e673e" +
        "656572726f72f6"
  }
}
