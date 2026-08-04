package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.client.data.ListUrlBuilder
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryListUrlParserBridgeTest {
  @Test
  fun encodesTheRustGeneratedRequestGoldenFixture() {
    val encodedHex =
      rustCbor.encodeToByteArray(goldenRequest()).joinToString(separator = "") {
        it.toUByte().toString(16).padStart(2, '0')
      }

    assertEquals(V1_REQUEST_GOLDEN_CBOR_HEX, encodedHex)
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedResponseGoldenFixture() {
    val envelope =
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryListUrlParsePlan>>(
        V1_RESPONSE_GOLDEN_CBOR_HEX.decodeHex()
      )

    assertEquals(goldenState(), envelope.toGalleryListUrlState())
  }

  @Test
  fun invokesTheSelectedNativeBoundaryWithTheCompleteSanitizedSnapshot() {
    val request = completeRequest()
    val expectedState = completeState()
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(
          version = 1,
          module = "galleryListUrlParser",
          payload = GalleryListUrlParsePlan(expectedState),
        )
      )
    var invocationCount = 0

    val actual =
      parseGalleryListUrl(request) { requestBuffer, requestSize, responseBuffer ->
        invocationCount++
        val requestBytes = ByteArray(requestSize)
        requestBuffer.get(requestBytes)
        assertEquals(
          request,
          rustCbor.decodeFromByteArray<GalleryListUrlParseRequest>(requestBytes),
        )
        responseBuffer.put(response)
        response.size
      }

    assertEquals(1, invocationCount)
    assertEquals(expectedState, actual)
  }

  @Test
  fun roundTripsEveryLegacyBuilderField() {
    val state = completeState()

    assertEquals(state, ListUrlBuilder.fromGalleryListUrlState(state).toGalleryListUrlState())
  }

  @Test
  fun rejectsWrongEnvelopeTypedFailureAndMissingPayload() {
    val plan = GalleryListUrlParsePlan(goldenState())
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryListUrlParser", payload = plan)
        .toGalleryListUrlState()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryListUrl", payload = plan).toGalleryListUrlState()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<GalleryListUrlParsePlan>(version = 1, module = "galleryListUrlParser")
        .toGalleryListUrlState()
    }
    assertNull(
      ApiEnvelope(
          version = 1,
          module = "galleryListUrlParser",
          payload = GalleryListUrlParsePlan(null),
        )
        .toGalleryListUrlState()
    )

    val failure = RustError(RustErrorKind.ERROR, "invalid encoded gallery-list path value")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryListUrlParsePlan>(
            version = 1,
            module = "galleryListUrlParser",
            error = failure,
          )
          .toGalleryListUrlState()
      }
    assertEquals(failure, error.failure)
  }

  private fun goldenRequest() =
    GalleryListUrlParseRequest(
      version = 1,
      snapshot =
        GalleryListUrlSnapshot(
          segments = listOf("tag", "artist%3Aalice"),
          parameters = emptyParameters(),
        ),
    )

  private fun completeRequest() =
    GalleryListUrlParseRequest(
      version = 1,
      snapshot =
        GalleryListUrlSnapshot(
          segments = emptyList(),
          parameters =
            GalleryListUrlParameters(
              imageHash = "hash",
              previous = "10",
              next = "20",
              seek = "2026-08-03",
              range = "15",
              excludedCategories = "767",
              search = "cats",
              advancedSearch = "1",
              searchName = "on",
              searchTags = "on",
              searchDescription = "on",
              searchTorrentFilenames = "on",
              searchLowPowerTags = "on",
              minimumRating = "4",
              pageFrom = "10",
              pageTo = "200",
              toplist = "11",
              toplistPage = "2",
            ),
        ),
    )

  private fun emptyParameters() =
    GalleryListUrlParameters(
      imageHash = null,
      previous = null,
      next = null,
      seek = null,
      range = null,
      excludedCategories = null,
      search = null,
      advancedSearch = null,
      searchName = null,
      searchTags = null,
      searchDescription = null,
      searchTorrentFilenames = null,
      searchLowPowerTags = null,
      minimumRating = null,
      pageFrom = null,
      pageTo = null,
      toplist = null,
      toplistPage = null,
    )

  private fun goldenState() =
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

  private fun completeState() =
    GalleryListUrlState(
      mode = 4,
      previous = "10",
      next = "20",
      seek = "2026-08-03",
      range = 15,
      category = 256,
      keyword = "cats",
      imageHash = "hash",
      language = -1,
      advancedSearch = 31,
      minimumRating = 4,
      pageFrom = 10,
      pageTo = 200,
    )

  private companion object {
    const val V1_REQUEST_GOLDEN_CBOR_HEX =
      "a26776657273696f6e0168736e617073686f74a2687365676d656e747382637461676e617274" +
        "697374253341616c6963656a706172616d6574657273b269696d61676548617368f668707265" +
        "76696f7573f6646e657874f6647365656bf66572616e6765f6726578636c7564656443617465" +
        "676f72696573f666736561726368f66e616476616e636564536561726368f66a736561726368" +
        "4e616d65f66a73656172636854616773f6717365617263684465736372697074696f6ef67673" +
        "6561726368546f7272656e7446696c656e616d6573f6727365617263684c6f77506f77657254" +
        "616773f66d6d696e696d756d526174696e67f6687061676546726f6df66670616765546ff667" +
        "746f706c697374f66b746f706c69737450616765f6"
    const val V1_RESPONSE_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657467616c6c6572794c69737455726c506172736572" +
        "677061796c6f6164a1657374617465ad646d6f6465026870726576696f7573f6646e657874f6" +
        "647365656bf66572616e6765006863617465676f727920676b6579776f72646c617274697374" +
        "3a616c69636569696d61676548617368f6686c616e6775616765206e616476616e6365645365" +
        "61726368206d6d696e696d756d526174696e6720687061676546726f6d206670616765546f20" +
        "656572726f72f6"
  }
}
