package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryFilterBridgeTest {
  @Test
  fun requestAndResponseGoldensMatchRust() {
    assertEquals(REQUEST_CBOR_HEX, rustCbor.encodeToByteArray(preRequest).toHex())
    assertEquals(
      preEnvelope,
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryFilterPlan>>(RESPONSE_CBOR_HEX.decodeHex()),
    )
    assertSharedGolden(fixtures[1], POST_REQUEST_CBOR_HEX, POST_RESPONSE_CBOR_HEX)
    assertSharedGolden(
      fixtures[2],
      COMMENT_PREFLIGHT_REQUEST_CBOR_HEX,
      COMMENT_PREFLIGHT_RESPONSE_CBOR_HEX,
    )
    assertSharedGolden(
      fixtures[3],
      COMMENT_FINAL_REQUEST_CBOR_HEX,
      COMMENT_FINAL_RESPONSE_CBOR_HEX,
    )
  }

  private fun assertSharedGolden(
    fixture: Pair<GalleryFilterRequest, GalleryFilterPlan>,
    requestHex: String,
    responseHex: String,
  ) {
    assertEquals(requestHex, rustCbor.encodeToByteArray(fixture.first).toHex())
    assertEquals(
      ApiEnvelope(1, GALLERY_FILTER_MODULE, fixture.second),
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryFilterPlan>>(responseHex.decodeHex()),
    )
  }

  @Test
  fun eachCompleteOperationUsesExactlyOneNativeCall() {
    fixtures.forEach { (request, plan) ->
      var calls = 0

      val actual =
        planGalleryFilter(request) { requestBuffer, requestSize, responseBuffer ->
          calls += 1
          val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
          assertEquals(request, rustCbor.decodeFromByteArray(requestBytes))
          val response = rustCbor.encodeToByteArray(ApiEnvelope(1, GALLERY_FILTER_MODULE, plan))
          responseBuffer.put(response)
          response.size
        }

      assertEquals(plan, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun validatorRejectsVersionModuleOperationAndEveryPlanLengthDrift() {
    assertFailsWith<IllegalArgumentException> {
      preEnvelope.copy(version = 2).toGalleryFilterPlan(preRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      preEnvelope.copy(module = "other").toGalleryFilterPlan(preRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      preEnvelope
        .copy(payload = prePlan.copy(operation = GalleryFilterOperation.POST_ENRICHMENT))
        .toGalleryFilterPlan(preRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      preEnvelope.copy(payload = prePlan.copy(keep = emptyList())).toGalleryFilterPlan(preRequest)
    }
    assertFailsWith<RustFailureException> {
      ApiEnvelope<GalleryFilterPlan>(
          version = 1,
          module = GALLERY_FILTER_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toGalleryFilterPlan(preRequest)
    }

    val commentRequest = fixtures.last().first
    val commentPlan = fixtures.last().second
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(1, GALLERY_FILTER_MODULE, commentPlan.copy(keep = emptyList()))
        .toGalleryFilterPlan(commentRequest)
    }

    val preflightRequest = fixtures[2].first
    val preflightPlan = fixtures[2].second
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(1, GALLERY_FILTER_MODULE, preflightPlan.copy(keep = listOf(true, true)))
        .toGalleryFilterPlan(preflightRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          1,
          GALLERY_FILTER_MODULE,
          preflightPlan.copy(commentEvaluationFilterIndices = listOf(emptyList())),
        )
        .toGalleryFilterPlan(preflightRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          1,
          GALLERY_FILTER_MODULE,
          preflightPlan.copy(commentEvaluationFilterIndices = listOf(emptyList(), listOf(1, 1))),
        )
        .toGalleryFilterPlan(preflightRequest)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          1,
          GALLERY_FILTER_MODULE,
          preflightPlan.copy(commentEvaluationFilterIndices = listOf(emptyList(), listOf(0))),
        )
        .toGalleryFilterPlan(preflightRequest)
    }
  }

  @Test
  fun requestRejectsOperationAndFactVectorLengthDriftBeforeNativeCall() {
    assertFailsWith<IllegalArgumentException> {
      GalleryFilterRequest(
        1,
        GalleryFilterOperation.PRE_ENRICHMENT,
        emptyList(),
        emptyList(),
        listOf(GalleryFilterComment(1, false, null)),
        null,
        null,
      )
    }
    assertFailsWith<IllegalArgumentException> {
      GalleryFilterRequest.comments(
        filters = listOf(GalleryFilterSnapshot(5, "pattern", true)),
        comments = listOf(GalleryFilterComment(1, false, null)),
        scoreThreshold = 0,
        commentContentMatches = emptyList(),
      )
    }
    assertFailsWith<IllegalArgumentException> {
      GalleryFilterRequest.comments(
        filters = listOf(GalleryFilterSnapshot(5, "pattern", true)),
        comments = listOf(GalleryFilterComment(1, false, null)),
        scoreThreshold = 0,
        commentContentMatches = listOf(emptyList()),
      )
    }
  }

  private companion object {
    val preRequest =
      GalleryFilterRequest.galleries(
        GalleryFilterOperation.PRE_ENRICHMENT,
        listOf(GalleryFilterSnapshot(0, "Needle", true)),
        listOf(GalleryFilterGallery("Title", "user", null)),
      )
    val prePlan =
      GalleryFilterPlan(
        GalleryFilterOperation.PRE_ENRICHMENT,
        listOf(true),
        false,
        emptyList(),
      )
    val preEnvelope = ApiEnvelope(1, GALLERY_FILTER_MODULE, prePlan)
    val fixtures =
      listOf(
        preRequest to prePlan,
        GalleryFilterRequest.galleries(
          GalleryFilterOperation.POST_ENRICHMENT,
          listOf(GalleryFilterSnapshot(2, "female:tag", true)),
          listOf(GalleryFilterGallery(null, null, listOf("female:tag", "group:value"))),
        ) to
          GalleryFilterPlan(
            GalleryFilterOperation.POST_ENRICHMENT,
            listOf(false),
            true,
            emptyList(),
          ),
        GalleryFilterRequest.comments(
          filters =
            listOf(
              GalleryFilterSnapshot(4, "alice", true),
              GalleryFilterSnapshot(5, "pattern", true),
            ),
          comments =
            listOf(
              GalleryFilterComment(1, true, "alice"),
              GalleryFilterComment(20, false, "bob"),
            ),
          scoreThreshold = 10,
        ) to
          GalleryFilterPlan(
            GalleryFilterOperation.COMMENT,
            emptyList(),
            false,
            listOf(emptyList(), listOf(1)),
          ),
        GalleryFilterRequest.comments(
          filters =
            listOf(
              GalleryFilterSnapshot(4, "alice", true),
              GalleryFilterSnapshot(5, "pattern", true),
            ),
          comments =
            listOf(
              GalleryFilterComment(1, true, "alice"),
              GalleryFilterComment(20, false, "bob"),
            ),
          scoreThreshold = 10,
          commentContentMatches = listOf(listOf(null, null), listOf(null, false)),
        ) to
          GalleryFilterPlan(
            GalleryFilterOperation.COMMENT,
            listOf(true, true),
            false,
            emptyList(),
          ),
      )

    const val REQUEST_CBOR_HEX =
      "a76776657273696f6e01696f7065726174696f6e6d707265456e726963686d656e746766696c7465727381a3646d6f6465006474657874664e6565646c6567656e61626c6564f56967616c6c657269657381a3657469746c65655469746c656875706c6f6164657264757365726474616773f668636f6d6d656e7473806e73636f72655468726573686f6c64f675636f6d6d656e74436f6e74656e744d617463686573f6"
    const val RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e6d707265456e726963686d656e74646b65657081f5696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
    const val POST_REQUEST_CBOR_HEX =
      "a76776657273696f6e01696f7065726174696f6e6e706f7374456e726963686d656e746766696c7465727381a3646d6f64650264746578746a66656d616c653a74616767656e61626c6564f56967616c6c657269657381a3657469746c65f66875706c6f61646572f66474616773826a66656d616c653a7461676b67726f75703a76616c756568636f6d6d656e7473806e73636f72655468726573686f6c64f675636f6d6d656e74436f6e74656e744d617463686573f6"
    const val POST_RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e6e706f7374456e726963686d656e74646b65657081f4696e6565647354616773f5781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
    const val COMMENT_PREFLIGHT_REQUEST_CBOR_HEX =
      "a76776657273696f6e01696f7065726174696f6e67636f6d6d656e746766696c7465727382a3646d6f646504647465787465616c69636567656e61626c6564f5a3646d6f6465056474657874677061747465726e67656e61626c6564f56967616c6c65726965738068636f6d6d656e747382a36573636f7265016875706c6f61646572f569636f6d6d656e74657265616c696365a36573636f7265146875706c6f61646572f469636f6d6d656e74657263626f626e73636f72655468726573686f6c640a75636f6d6d656e74436f6e74656e744d617463686573f6"
    const val COMMENT_PREFLIGHT_RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e67636f6d6d656e74646b65657080696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657382808101656572726f72f6"
    const val COMMENT_FINAL_REQUEST_CBOR_HEX =
      "a76776657273696f6e01696f7065726174696f6e67636f6d6d656e746766696c7465727382a3646d6f646504647465787465616c69636567656e61626c6564f5a3646d6f6465056474657874677061747465726e67656e61626c6564f56967616c6c65726965738068636f6d6d656e747382a36573636f7265016875706c6f61646572f569636f6d6d656e74657265616c696365a36573636f7265146875706c6f61646572f469636f6d6d656e74657263626f626e73636f72655468726573686f6c640a75636f6d6d656e74436f6e74656e744d6174636865738282f6f682f6f4"
    const val COMMENT_FINAL_RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d67616c6c65727946696c746572677061796c6f6164a4696f7065726174696f6e67636f6d6d656e74646b65657082f5f5696e6565647354616773f4781e636f6d6d656e744576616c756174696f6e46696c746572496e646963657380656572726f72f6"
  }
}

private fun ByteArray.toHex() = joinToString("") { byte -> "%02x".format(byte) }

private fun String.decodeHex() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
