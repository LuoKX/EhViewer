package com.hippo.ehviewer.client.parser

import com.ehviewer.core.model.GalleryInfo
import com.hippo.ehviewer.bridge.GalleryMetadataBatch
import com.hippo.ehviewer.bridge.GalleryMetadataItem
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.SerializationException

class GalleryApiParserTest {
  @Test
  fun legacyParserPreservesFrozenNormalizationAndMissingNullableUploader() {
    val target = FakeGalleryInfo(gid = 7)
    val body =
      """{"gmetadata":[{"gid":7,"title":"&amp;lt; &unknown;","title_jpn":"&lt;日本&gt;","category":"aRtIsT cG SeTs","thumb":"https://ehgt.org/t/a.jpg","posted":1700000000,"filecount":12,"rating":4.75,"tags":["language:english"],"ignored":true}],"ignored":true}"""

    GalleryApiParser.parse(body, listOf(target))

    assertEquals("&lt; &unknown;", target.title)
    assertEquals("<日本>", target.titleJpn)
    assertEquals(0x8, target.category)
    assertEquals("t/a.jpg", target.thumbKey)
    assertNull(target.uploader)
    assertEquals(ParserUtils.formatDate(1_700_000_000_000), target.posted)
    assertEquals(4.75F, target.rating)
    assertEquals(listOf("language:english"), target.simpleTags)
    assertEquals(12, target.pages)
    assertEquals("EN", target.simpleLanguage)
  }

  @Test
  fun legacyParserRejectsMalformedMissingNullAndWrongRequiredFields() {
    listOf(
        """{"gmetadata":[{}]}""",
        """{"gmetadata":[{"gid":1,"title":null}]}""",
        """{"gmetadata":{}}""",
        """{"gmetadata":null}""",
        """{"gmetadata":["wrong"]}""",
        """{"gmetadata":[{"gid":1""",
      )
      .forEach { body ->
        assertFailsWith<SerializationException> {
          GalleryApiParser.parse(body, listOf(FakeGalleryInfo(gid = 1)))
        }
      }
  }

  @Test
  fun legacyParserUsesLastDuplicateFieldsAndSourceOrderedFirstTargetMutation() {
    val firstTarget = FakeGalleryInfo(gid = 7)
    val duplicateTarget = FakeGalleryInfo(gid = 7, title = "untouched")
    val body =
      """{"gmetadata":[{"gid":7,"title":"old","title":"first","title_jpn":"","category":"misc","thumb":"one","uploader":null,"posted":0,"filecount":1,"rating":1,"tags":[]},{"gid":99,"title":"skip","title_jpn":"","category":"misc","thumb":"skip","uploader":null,"posted":0,"filecount":1,"rating":1,"tags":[]},{"gid":7,"title":"second","title_jpn":"","category":"western","thumb":"two","uploader":null,"posted":0,"filecount":2,"rating":2,"tags":[]}]}"""

    GalleryApiParser.parse(body, listOf(firstTarget, duplicateTarget))

    assertEquals("second", firstTarget.title)
    assertEquals(0x200, firstTarget.category)
    assertEquals("two", firstTarget.thumbKey)
    assertEquals(2, firstTarget.pages)
    assertEquals("untouched", duplicateTarget.title)
  }

  @Test
  fun v1ApplicationKeepsSourceOrderFirstMatchAndAndroidPresentationOwnership() {
    val firstTarget = FakeGalleryInfo(gid = 7)
    val duplicateTarget = FakeGalleryInfo(gid = 7, title = "untouched")
    val batch =
      GalleryMetadataBatch(
        listOf(
          metadata(galleryId = 99, title = "skip"),
          metadata(galleryId = 7, title = "first", tags = listOf("language:english")),
          metadata(
            galleryId = 7,
            title = "second",
            titleJapanese = "二",
            category = 0x200,
            thumbnailKey = "two.jpg",
            uploader = "Uploader",
            posted = 1_700_000_001,
            rating = 3.5F,
            tags = listOf("language:chinese"),
            pageCount = 9,
          ),
        )
      )

    GalleryApiParser.applyV1(batch, listOf(firstTarget, duplicateTarget))

    assertEquals("second", firstTarget.title)
    assertEquals("二", firstTarget.titleJpn)
    assertEquals(0x200, firstTarget.category)
    assertEquals("two.jpg", firstTarget.thumbKey)
    assertEquals("Uploader", firstTarget.uploader)
    assertEquals(ParserUtils.formatDate(1_700_000_001_000), firstTarget.posted)
    assertEquals(3.5F, firstTarget.rating)
    assertEquals(listOf("language:chinese"), firstTarget.simpleTags)
    assertEquals(9, firstTarget.pages)
    assertEquals("ZH", firstTarget.simpleLanguage)
    assertEquals("untouched", duplicateTarget.title)
  }

  @Test
  fun defaultOffPathInvokesOnlyTheLegacyParser() = runBlocking {
    var legacyCalls = 0
    var rustCalls = 0

    GalleryApiParser.parseWithAdapters(
      useRust = false,
      legacyParser = { legacyCalls += 1 },
      rustParser = { rustCalls += 1 },
    )

    assertEquals(1, legacyCalls)
    assertEquals(0, rustCalls)
  }

  @Test
  fun enabledPathInvokesOnlyV1AndNeverReplaysAfterFailureOrCancellation() = runBlocking {
    var legacyCalls = 0
    var rustCalls = 0
    GalleryApiParser.parseWithAdapters(
      useRust = true,
      legacyParser = { legacyCalls += 1 },
      rustParser = { rustCalls += 1 },
    )
    assertEquals(0, legacyCalls)
    assertEquals(1, rustCalls)

    val failure = RustError(RustErrorKind.ERROR, "Gallery metadata response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        GalleryApiParser.parseWithAdapters(
          useRust = true,
          legacyParser = { legacyCalls += 1 },
          rustParser = { throw RustFailureException(failure) },
        )
      }
    assertEquals(failure, exception.failure)
    assertEquals(0, legacyCalls)

    assertFailsWith<CancellationException> {
      GalleryApiParser.parseWithAdapters(
        useRust = true,
        legacyParser = { legacyCalls += 1 },
        rustParser = { throw CancellationException("cancelled") },
      )
    }
    assertEquals(0, legacyCalls)
  }

  private fun metadata(
    galleryId: Long,
    title: String,
    titleJapanese: String = "",
    category: Int = 0x1,
    thumbnailKey: String = "thumb.jpg",
    uploader: String? = null,
    posted: Long = 0,
    rating: Float = 0F,
    tags: List<String> = emptyList(),
    pageCount: Int = 0,
  ) =
    GalleryMetadataItem(
      galleryId = galleryId,
      title = title,
      titleJapanese = titleJapanese,
      category = category,
      thumbnailKey = thumbnailKey,
      uploader = uploader,
      posted = posted,
      rating = rating,
      tags = tags,
      pageCount = pageCount,
    )

  private class FakeGalleryInfo(
    override var gid: Long,
    override var title: String? = null,
  ) : GalleryInfo {
    override var token: String = "token"
    override var titleJpn: String? = null
    override var thumbKey: String? = null
    override var category: Int = 0
    override var posted: String? = null
    override var uploader: String? = null
    override var disowned: Boolean = false
    override var rating: Float = 0F
    override var rated: Boolean = false
    override var simpleTags: List<String>? = null
    override var pages: Int = 0
    override var thumbWidth: Int = 0
    override var thumbHeight: Int = 0
    override var simpleLanguage: String? = null
    override var favoriteSlot: Int = GalleryInfo.NOT_FAVORITED
    override var favoriteName: String? = null
    override var favoriteNote: String? = null
  }
}
