package com.hippo.ehviewer.bridge.reader

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ReaderPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class ReaderPageBridgeTest {
  @Test
  fun mapsTheReaderContractToTheExistingParserResult() {
    val result = successfulEnvelope().toGalleryPageResult()

    assertEquals("https://img.example/page.jpg", result.imageUrl)
    assertEquals("skip", result.skipHathKey)
    assertEquals(null, result.originImageUrl)
    assertEquals("show", result.showKey)
  }

  @Test
  fun decodesTheRustGeneratedV1CborGoldenFixture() {
    val envelope = Cbor.decodeFromByteArray<ApiEnvelope<ReaderPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("readerPage", envelope.module)
    assertTrue(envelope.isOk())
    assertEquals("https://img.example/page.jpg", envelope.toGalleryPageResult().imageUrl)
  }

  @Test
  fun rejectsUnsupportedVersionUnexpectedModuleAndInvalidPayloads() {
    val page = successfulPage()
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "readerPage", payload = page).toGalleryPageResult()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryDetail", payload = page).toGalleryPageResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = page.copy(imageUrl = " ")).toGalleryPageResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = page.copy(skipHathKey = "")).toGalleryPageResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = page.copy(originalImageUrl = "")).toGalleryPageResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope().copy(payload = page.copy(showKey = "")).toGalleryPageResult()
    }
  }

  @Test
  fun preservesTypedRustFailureForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.IP_BANNED, "blocked")
    val envelope = ApiEnvelope<ReaderPage>(version = 1, module = "readerPage", error = failure)

    val error = assertFailsWith<RustFailureException> { envelope.toGalleryPageResult() }
    assertEquals(failure, error.failure)
  }

  private fun successfulEnvelope() =
    ApiEnvelope(version = 1, module = "readerPage", payload = successfulPage())

  private fun successfulPage() =
    ReaderPage(
      imageUrl = "https://img.example/page.jpg",
      skipHathKey = "skip",
      originalImageUrl = null,
      showKey = "show",
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656a72656164657250616765677061796c6f6164a468696d" +
        "61676555726c781c68747470733a2f2f696d672e6578616d706c652f706167652e6a70676b736b69" +
        "70486174684b657964736b6970706f726967696e616c496d61676555726cf66773686f774b65796473" +
        "686f77656572726f72f6"
  }
}
