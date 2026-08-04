package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.PowerStatus
import com.ehviewer.core.model.TagNamespace
import com.ehviewer.core.model.V2GalleryPreview
import com.ehviewer.core.model.VoteStatus
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryComment
import com.hippo.ehviewer.bridge.GalleryCommentsPage
import com.hippo.ehviewer.bridge.GalleryDetail
import com.hippo.ehviewer.bridge.GalleryDetailPage
import com.hippo.ehviewer.bridge.GalleryItemSummary
import com.hippo.ehviewer.bridge.GalleryPreview
import com.hippo.ehviewer.bridge.GalleryPreviewKind
import com.hippo.ehviewer.bridge.GalleryPreviewPage
import com.hippo.ehviewer.bridge.GalleryTag
import com.hippo.ehviewer.bridge.GalleryTagGroup
import com.hippo.ehviewer.bridge.GalleryTagPower
import com.hippo.ehviewer.bridge.GalleryTagVote
import com.hippo.ehviewer.bridge.GalleryVersionSummary
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryDetailBridgeTest {
  @Test
  fun mapsRichDetailTagsCommentsPreviewsAndEventWithoutFieldLoss() {
    val result = successfulEnvelope().toGalleryDetailResult()
    val detail = result.detail

    assertEquals("<strong>Event</strong>", result.event)
    assertEquals(42L, detail.gid)
    assertEquals("Title", detail.title)
    assertEquals("thumb.jpg", detail.thumbKey)
    assertEquals(12, detail.pages)
    assertEquals(-1L, detail.apiUid)
    assertEquals("api-key", detail.apiKey)
    assertEquals(3, detail.torrentCount)
    assertEquals("Yes", detail.visible)
    assertEquals("English", detail.language)
    assertEquals("12 MiB", detail.size)
    assertEquals(4, detail.favoriteCount)
    assertEquals(10, detail.ratingCount)
    assertEquals(43L, detail.newerVersions.single().gid)
    assertEquals(TagNamespace.Artist, detail.tagGroups.single().namespace)
    with(detail.tagGroups.single().tags.single()) {
      assertEquals("Alice", text)
      assertEquals(PowerStatus.Active, power)
      assertEquals(VoteStatus.Up, vote)
    }
    assertEquals(123L, detail.comments.comments.single().id)
    with(assertIs<V2GalleryPreview>(detail.previewList.single())) {
      assertEquals(16, offsetX)
      assertEquals(80, clipWidth)
      assertEquals(120, clipHeight)
    }
  }

  @Test
  fun decodesTheRustGeneratedV1CborGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryDetailPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    assertEquals(1, envelope.version)
    assertEquals("galleryDetail", envelope.module)
    assertTrue(envelope.isOk())
    assertEquals("Title", envelope.toGalleryDetailResult().detail.title)
  }

  @Test
  fun rejectsUnsupportedVersionModuleAndNestedContractDrift() {
    val page = completePage()
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryDetail", payload = page).toGalleryDetailResult()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = page).toGalleryDetailResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope()
        .copy(payload = page.copy(previews = page.previews.copy(total = 11)))
        .toGalleryDetailResult()
    }
    assertFailsWith<IllegalStateException> {
      successfulEnvelope()
        .copy(
          payload =
            page.copy(tagGroups = listOf(page.tagGroups.single().copy(namespace = "unknown")))
        )
        .toGalleryDetailResult()
    }
    assertFailsWith<IllegalArgumentException> {
      successfulEnvelope()
        .copy(
          payload =
            page.copy(
              tagGroups =
                listOf(
                  page.tagGroups
                    .single()
                    .copy(tags = listOf(GalleryTag(" ", GalleryTagPower.ACTIVE, GalleryTagVote.UP)))
                )
            )
        )
        .toGalleryDetailResult()
    }
  }

  @Test
  fun preservesTypedRustFailureForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.GALLERY_UNAVAILABLE, "removed")
    val envelope =
      ApiEnvelope<GalleryDetailPage>(
        version = 1,
        module = "galleryDetail",
        error = failure,
      )

    val error = assertFailsWith<RustFailureException> { envelope.toGalleryDetailResult() }
    assertEquals(failure, error.failure)
  }

  private fun successfulEnvelope() =
    ApiEnvelope(version = 1, module = "galleryDetail", payload = completePage())

  private fun completePage() =
    GalleryDetailPage(
      detail =
        GalleryDetail(
          galleryInfo = completeGalleryInfo(),
          apiUid = -1,
          apiKey = "api-key",
          torrentCount = 3,
          parent = "https://e-hentai.org/g/41/parent0001/",
          newerVersions =
            listOf(
              GalleryVersionSummary(
                galleryId = 43,
                token = "abcdef1234",
                title = "New version",
                posted = "2026-08-03",
              )
            ),
          visible = "Yes",
          language = "English",
          size = "12 MiB",
          favoriteCount = 4,
          ratingCount = 10,
        ),
      tagGroups =
        listOf(
          GalleryTagGroup(
            namespace = "artist",
            tags = listOf(GalleryTag("Alice", GalleryTagPower.ACTIVE, GalleryTagVote.UP)),
          )
        ),
      comments =
        GalleryCommentsPage(
          comments =
            listOf(
              GalleryComment(
                id = 123,
                score = 1,
                editable = false,
                canVoteUp = true,
                votedUp = false,
                canVoteDown = true,
                votedDown = false,
                uploader = false,
                postedAt = 1_785_678_300_000,
                user = "Alice",
                bodyHtml = "Comment",
                lastEditedAt = 0,
              )
            ),
          hasMore = true,
        ),
      previews =
        GalleryPreviewPage(
          previews =
            listOf(
              GalleryPreview(
                kind = GalleryPreviewKind.V2,
                url = "sprite.jpg",
                position = 0,
                pageToken = "token",
                offsetX = 16,
                clipWidth = 80,
                clipHeight = 120,
              )
            ),
          total = 12,
        ),
      eventHtml = "<strong>Event</strong>",
    )

  private fun completeGalleryInfo() =
    GalleryItemSummary(
      galleryId = 42,
      title = "Title",
      token = "0123456789",
      thumbnailKey = "thumb.jpg",
      category = 4,
      posted = "2026-08-02",
      disowned = false,
      rating = 4.5f,
      rated = false,
      pageCount = 12,
      thumbnailWidth = 0,
      thumbnailHeight = 0,
      favoriteSlot = -2,
    )

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656d67616c6c65727944657461696c677061796c6f6164a5" +
        "6664657461696cab6b67616c6c657279496e666fb363676964182a657469746c65655469746c6565" +
        "746f6b656e6a30313233343536373839687469746c654a706ef6687468756d624b6579697468756d" +
        "622e6a70676863617465676f72790466706f73746564606875706c6f61646572f6686469736f776e" +
        "6564f466726174696e67f94480657261746564f46a73696d706c655461677380657061676573016a" +
        "7468756d625769647468006b7468756d62486569676874006e73696d706c654c616e6775616765f6" +
        "6c6661766f72697465536c6f74216c6661766f726974654e616d65f66c6661766f726974654e6f74" +
        "65f66661706955696420666170694b6579676170692d6b65796c746f7272656e74436f756e740066" +
        "706172656e74f66d6e6577657256657273696f6e73806776697369626c65f6686c616e6775616765" +
        "f66473697a65f66d6661766f72697465436f756e74006b726174696e67436f756e74006974616747" +
        "726f7570738068636f6d6d656e7473a268636f6d6d656e747380676861734d6f7265f46870726576" +
        "69657773a268707265766965777381a7646b696e646276316375726c6b707265766965772e6a7067" +
        "68706f736974696f6e006670546f6b656e65746f6b656e676f666673657458f669636c6970576964" +
        "7468f66a636c6970486569676874f665746f74616c01696576656e7448746d6cf6656572726f72f6"
  }
}
