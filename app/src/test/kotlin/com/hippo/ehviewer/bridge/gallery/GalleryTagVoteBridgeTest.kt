package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.PowerStatus
import com.ehviewer.core.model.TagNamespace
import com.ehviewer.core.model.VoteStatus
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryTag
import com.hippo.ehviewer.bridge.GalleryTagGroup
import com.hippo.ehviewer.bridge.GalleryTagPower
import com.hippo.ehviewer.bridge.GalleryTagVote
import com.hippo.ehviewer.bridge.GalleryTags
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryTagVoteBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixtureWithoutLosingOrderOrEmptyText() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryTags>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val groups = envelope.toGalleryTagVoteResult()
    assertEquals(2, groups.size)
    assertEquals(TagNamespace.Artist, groups[0].namespace)
    assertEquals("Alice & Bob", groups[0].tags[0].text)
    assertEquals(PowerStatus.Active, groups[0].tags[0].power)
    assertEquals(VoteStatus.Up, groups[0].tags[0].vote)
    assertEquals("", groups[0].tags[1].text)
    assertEquals(PowerStatus.Weak, groups[0].tags[1].power)
    assertEquals(VoteStatus.None, groups[0].tags[1].vote)
    assertEquals(TagNamespace.Language, groups[1].namespace)
    assertEquals(PowerStatus.Solid, groups[1].tags.single().power)
    assertEquals(VoteStatus.Down, groups[1].tags.single().vote)
  }

  @Test
  fun rejectsVersionModuleAndNamespaceDriftButAllowsEmptyTagText() {
    val envelope = successEnvelope(namespace = "artist", text = "")
    assertEquals("", envelope.toGalleryTagVoteResult().single().tags.single().text)

    assertFailsWith<IllegalArgumentException> {
      envelope.copy(version = 2).toGalleryTagVoteResult()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(module = "galleryToken").toGalleryTagVoteResult()
    }
    assertFailsWith<IllegalStateException> {
      successEnvelope(namespace = "unknown", text = "tag").toGalleryTagVoteResult()
    }
  }

  @Test
  fun preservesTypedRustFailuresForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "The tag is blocked; try artist:example")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryTags>(version = 1, module = "galleryTagVote", error = failure)
          .toGalleryTagVoteResult()
      }

    assertEquals(failure, exception.failure)
  }

  private fun successEnvelope(namespace: String, text: String) =
    ApiEnvelope(
      version = 1,
      module = "galleryTagVote",
      payload =
        GalleryTags(
          groups =
            listOf(
              GalleryTagGroup(
                namespace = namespace,
                tags = listOf(GalleryTag(text, GalleryTagPower.WEAK, GalleryTagVote.NONE)),
              )
            )
        ),
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e67616c6c657279546167566f7465" +
        "677061796c6f6164a16667726f75707382a2696e616d6573706163656661727469" +
        "7374647461677382a364746578746b416c696365202620426f6265706f77657266" +
        "61637469766564766f7465627570a364746578746065706f776572647765616b64" +
        "766f7465646e6f6e65a2696e616d657370616365686c616e677561676564746167" +
        "7381a3647465787467656e676c69736865706f77657265736f6c696464766f7465" +
        "64646f776e656572726f72f6"
  }
}
