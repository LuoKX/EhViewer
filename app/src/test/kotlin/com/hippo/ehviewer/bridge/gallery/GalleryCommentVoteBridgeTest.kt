package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryCommentVote
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.client.parser.VoteCommentResult
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryCommentVoteBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryCommentVote>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(voteCommentResultFixture, envelope.toVoteCommentResult())
  }

  @Test
  fun preservesAllSignedFieldsWithoutIdentityOrVotePolicy() {
    val minimum =
      successEnvelope(id = Long.MIN_VALUE, score = Int.MIN_VALUE, vote = Int.MIN_VALUE)
        .toVoteCommentResult()
    assertEquals(Long.MIN_VALUE, minimum.id)
    assertEquals(Int.MIN_VALUE, minimum.score)
    assertEquals(Int.MIN_VALUE, minimum.vote)

    val maximum =
      successEnvelope(id = Long.MAX_VALUE, score = Int.MAX_VALUE, vote = Int.MAX_VALUE)
        .toVoteCommentResult()
    assertEquals(Long.MAX_VALUE, maximum.id)
    assertEquals(Int.MAX_VALUE, maximum.score)
    assertEquals(Int.MAX_VALUE, maximum.vote)
  }

  @Test
  fun rejectsVersionAndModuleDrift() {
    val envelope = successEnvelope(id = 1, score = 2, vote = 1)

    assertFailsWith<IllegalArgumentException> {
      envelope.copy(version = 2).toVoteCommentResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(module = "galleryRate").toVoteCommentResult()
    }
  }

  @Test
  fun preservesTypedRustFailuresForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "Gallery comment vote response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryCommentVote>(
            version = 1,
            module = "galleryCommentVote",
            error = failure,
          )
          .toVoteCommentResult()
      }

    assertEquals(failure, exception.failure)
  }

  private fun successEnvelope(id: Long, score: Int, vote: Int) =
    ApiEnvelope(
      version = 1,
      module = "galleryCommentVote",
      payload = GalleryCommentVote(id = id, score = score, vote = vote),
    )

  private companion object {
    val voteCommentResultFixture = VoteCommentResult(id = 1_234_567_890_123, score = -7, vote = 1)

    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657267616c6c657279436f6d6d656e74566f7465" +
        "677061796c6f6164a36269641b0000011f71fb04cb6573636f72652664766f746501656572" +
        "726f72f6"
  }
}
