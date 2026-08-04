package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryComment
import com.hippo.ehviewer.bridge.GalleryCommentsPage
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryCommentsBridgeTest {
  @Test
  fun mapsEveryRustFieldToTheExistingAndroidModel() {
    val result = successfulEnvelope().toGalleryCommentList()

    assertTrue(result.hasMore)
    with(result.comments.single()) {
      assertEquals(123L, id)
      assertEquals(-7, score)
      assertTrue(editable)
      assertTrue(voteUpAble)
      assertTrue(voteUpEd)
      assertTrue(voteDownAble)
      assertFalse(voteDownEd)
      assertFalse(uploader)
      assertEquals("Voted & noted", voteState)
      assertEquals(1_785_678_300_000, time)
      assertEquals("Alice & Bob", user)
      assertEquals("First <u>underlined</u>", comment)
      assertEquals(1_785_767_400_000, lastEdited)
    }
  }

  @Test
  fun decodesTheRustGeneratedV1CborGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryCommentsPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("galleryComments", envelope.module)
    assertTrue(envelope.isOk())
    assertEquals(123L, envelope.toGalleryCommentList().comments.single().id)
  }

  @Test
  fun acceptsAnEmptyCommentPage() {
    val envelope =
      ApiEnvelope(
        version = 1,
        module = "galleryComments",
        payload = GalleryCommentsPage(hasMore = false),
      )

    val result = envelope.toGalleryCommentList()
    assertTrue(result.comments.isEmpty())
    assertFalse(result.hasMore)
  }

  @Test
  fun rejectsUnsupportedVersionAndUnexpectedModule() {
    val page = GalleryCommentsPage(hasMore = false)
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryComments", payload = page).toGalleryCommentList()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryDetail", payload = page).toGalleryCommentList()
    }
  }

  @Test
  fun preservesTypedRustFailureForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "Comment access denied")
    val envelope =
      ApiEnvelope<GalleryCommentsPage>(
        version = 1,
        module = "galleryComments",
        error = failure,
      )

    val error = assertFailsWith<RustFailureException> { envelope.toGalleryCommentList() }
    assertEquals(failure, error.failure)
  }

  @Test
  fun rejectsInvalidCommentState() {
    val invalidComments =
      listOf(
        completeComment().copy(id = -1),
        completeComment().copy(postedAt = 0),
        completeComment().copy(lastEditedAt = -1),
        completeComment().copy(uploader = true),
        completeComment().copy(canVoteUp = false, votedUp = true),
        completeComment().copy(canVoteDown = false, votedDown = true),
      )

    for (comment in invalidComments) {
      assertFailsWith<IllegalArgumentException> {
        commentsEnvelope(comment).toGalleryCommentList()
      }
    }
  }

  private fun successfulEnvelope() = commentsEnvelope(completeComment(), hasMore = true)

  private fun commentsEnvelope(comment: GalleryComment, hasMore: Boolean = false) =
    ApiEnvelope(
      version = 1,
      module = "galleryComments",
      payload = GalleryCommentsPage(comments = listOf(comment), hasMore = hasMore),
    )

  private fun completeComment() =
    GalleryComment(
      id = 123,
      score = -7,
      editable = true,
      canVoteUp = true,
      votedUp = true,
      canVoteDown = true,
      votedDown = false,
      uploader = false,
      voteState = "Voted & noted",
      postedAt = 1_785_678_300_000,
      user = "Alice & Bob",
      bodyHtml = "First <u>underlined</u>",
      lastEditedAt = 1_785_767_400_000,
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656f67616c6c657279436f6d6d656e7473677061796c6f" +
        "6164a268636f6d6d656e747381ad626964187b6573636f726526686564697461626c65f56a766f74" +
        "65557041626c65f568766f746555704564f56c766f7465446f776e41626c65f56a766f7465446f77" +
        "6e4564f46875706c6f61646572f469766f746553746174656d566f7465642026206e6f7465646474" +
        "696d651b0000019fc2b8836064757365726b416c696365202620426f6267636f6d6d656e74774669" +
        "727374203c753e756e6465726c696e65643c2f753e6a6c6173744564697465641b0000019fc80812" +
        "40676861734d6f7265f5656572726f72f6"
  }
}
