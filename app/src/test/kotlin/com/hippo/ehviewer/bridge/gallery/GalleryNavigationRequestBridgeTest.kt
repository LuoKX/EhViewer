package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.client.ehUrl
import io.ktor.http.Parameters
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray
import kotlinx.serialization.json.Json

class GalleryNavigationRequestBridgeTest {
  @Serializable
  private data class Fixture(
    val name: String,
    @SerialName("request") val wireRequest: GalleryRequest,
    val url: String,
    val tag: String? = null,
  ) {
    val request: GalleryRequest
      get() =
        if (wireRequest.kind == GalleryRequestKind.TAG_DEFINITION_URL)
          wireRequest.copy(wikiTagUtf16 = checkNotNull(tag).toUtf16LeBytes())
        else wireRequest
  }

  @Serializable private data class WikiFixture(val name: String, val units: List<Int>)

  @Serializable
  private data class UncheckedEnvelope(
    val version: Int,
    val module: String,
    val payload: GalleryRequestPlan?,
    val error: RustError?,
  )

  private val root = File("src/main/rust/crates/net/test_data")

  private fun fixtures() =
    Json.decodeFromString<List<Fixture>>(root.resolve("navigation_urls.json").readText())

  private fun bytes(
    name: String,
    suffix: String,
  ) =
    root
      .resolve("navigation_${name}_$suffix.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  private fun plan(fixture: Fixture) =
    GalleryRequestPlan(fixture.request.kind, fixture.url, null, null, emptyList(), null, false)

  private fun wirePlan(fixture: Fixture): GalleryRequestPlan =
    if (fixture.request.kind == GalleryRequestKind.TAG_DEFINITION_URL)
      plan(fixture).copy(url = "", wikiUrlUtf16 = fixture.url.toUtf16LeBytes(), wikiSourceIndex = 0)
    else plan(fixture)

  @Test
  fun wikiRawGoldensPreserveEveryJavaCodeUnitAndUseOneNativePlan() {
    val fixtures =
      Json.decodeFromString<List<WikiFixture>>(
        root.resolve("navigation_wiki_utf16.json").readText()
      )
    for (fixture in fixtures) {
      val tag = String(CharArray(fixture.units.size) { fixture.units[it].toChar() })
      val oracle = "https://ehwiki.org/wiki/" + tag.replace(' ', '_')
      val request =
        GalleryRequest(
          1,
          GalleryUrlSite.PUBLIC,
          GalleryRequestKind.TAG_DEFINITION_URL,
          wikiTagUtf16 = tag.toUtf16LeBytes(),
        )
      assertEquals(
        bytes(fixture.name, "request").toList(),
        rustCbor.encodeToByteArray(request).toList(),
      )
      var calls = 0
      val actual =
        planGalleryRequest(request) { _, _ ->
          calls++
          bytes(fixture.name, "response")
        }
      assertEquals(oracle.toCharArray().toList(), actual.url.toCharArray().toList())
      assertEquals(null, actual.wikiUrlUtf16)
      assertEquals(null, actual.wikiSourceIndex)
      assertEquals(1, calls)
    }
  }

  @Test
  fun wikiRawShapePrefixLengthSourceAndBudgetsAreValidatedBeforeMapping() {
    val request =
      GalleryRequest(
        1,
        GalleryUrlSite.PUBLIC,
        GalleryRequestKind.TAG_DEFINITION_URL,
        wikiTagUtf16 = "\uD800".toUtf16LeBytes(),
      )
    val valid =
      rustCbor
        .decodeFromByteArray<ApiEnvelope<GalleryRequestPlan>>(bytes("wiki_high", "response"))
        .payload!!
    val raw = valid.wikiUrlUtf16!!
    for (invalid in
      listOf(
        valid.copy(url = "https://ehwiki.org/wiki/legacy"),
        valid.copy(wikiUrlUtf16 = null),
        valid.copy(wikiSourceIndex = null),
        valid.copy(wikiSourceIndex = -1),
        valid.copy(wikiSourceIndex = 1),
        valid.copy(wikiUrlUtf16 = raw.copyOf(raw.size - 1)),
        valid.copy(wikiUrlUtf16 = raw.copyOf(raw.size + 2)),
        valid.copy(wikiUrlUtf16 = raw.copyOf().also { it[0] = 0 }),
        valid.copy(wikiUrlUtf16 = ByteArray(131_122)),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(request) { _, _ ->
          rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", invalid))
        }
      }
    }
    for (invalid in
      listOf(
        request.copy(wikiTagUtf16 = null),
        request.copy(wikiTagUtf16 = byteArrayOf(0)),
        request.copy(wikiTagUtf16 = ByteArray(131_074)),
        request.copy(token = "secret-token"),
        request.copy(key = "secret-key"),
        request.copy(kind = GalleryRequestKind.TORRENT_URL),
      )) {
      var calls = 0
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(invalid) { _, _ ->
          calls++
          error("must not call")
        }
      }
      assertEquals(0, calls)
    }
    val nonWiki = GalleryRequest(1, GalleryUrlSite.PUBLIC, GalleryRequestKind.TORRENT_URL)
    val extraRaw =
      valid.copy(kind = nonWiki.kind, url = "https://e-hentai.org/gallerytorrents.php?gid=0&t=")
    assertFailsWith<IllegalArgumentException> {
      planGalleryRequest(nonWiki) { _, _ ->
        rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", extraRaw))
      }
    }
    val maximum = request.copy(wikiTagUtf16 = "\uD800".repeat(65_536).toUtf16LeBytes())
    val maximumTarget = "https://ehwiki.org/wiki/" + "\uD800".repeat(65_536)
    val mapped =
      planGalleryRequest(maximum) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(
            1,
            "galleryRequest",
            valid.copy(wikiUrlUtf16 = maximumTarget.toUtf16LeBytes()),
          )
        )
      }
    assertEquals(maximumTarget.toCharArray().toList(), mapped.url.toCharArray().toList())
  }

  @Test
  fun settingSnapshotKeepsUnknownFallbackAndBothSites() {
    assertEquals(GalleryUrlSite.EX_HENTAI, galleryRequestSite(1))
    for (site in listOf(0, -1, 2, Int.MIN_VALUE, Int.MAX_VALUE)) {
      assertEquals(GalleryUrlSite.PUBLIC, galleryRequestSite(site))
    }
  }

  @Test
  fun maximumUtf8BudgetAndEncodedResponseAreAcceptedWithoutRegexStackGrowth() {
    val request =
      GalleryRequest(
        1,
        GalleryUrlSite.PUBLIC,
        GalleryRequestKind.TRACKER_URL,
        gid = Long.MIN_VALUE,
        key = "😀".repeat(16_384),
      )
    val plan =
      GalleryRequestPlan(
        request.kind,
        "https://ehtracker.org/${Long.MIN_VALUE}/" + "%F0%9F%98%80".repeat(16_384) + "/announce",
        null,
        null,
        emptyList(),
        null,
        false,
      )
    assertEquals(
      plan,
      planGalleryRequest(request) { _, _ ->
        rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", plan))
      },
    )
  }

  @Test
  fun sharedExactCborRequestsAndResponseUrlsUseOneCall() {
    for (fixture in fixtures().take(6)) {
      assertEquals(
        bytes(fixture.name, "request").toList(),
        rustCbor.encodeToByteArray(fixture.request).toList(),
      )
      var calls = 0
      val actual =
        planGalleryRequest(fixture.request) { buffer, size ->
          calls++
          assertTrue(buffer.isDirect)
          val decoded =
            rustCbor.decodeFromByteArray<GalleryRequest>(ByteArray(size).also(buffer::get))
          assertEquals(fixture.request.copy(wikiTagUtf16 = null), decoded.copy(wikiTagUtf16 = null))
          assertEquals(fixture.request.wikiTagUtf16?.toList(), decoded.wikiTagUtf16?.toList())
          bytes(fixture.name, "response")
        }
      assertEquals(plan(fixture), actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun independentFrozenKtorAndRawWikiCharacterizationMatchesEveryFixture() {
    for (fixture in fixtures()) {
      val request = fixture.request
      val host = if (request.site == GalleryUrlSite.EX_HENTAI) "exhentai.org" else "e-hentai.org"
      val legacy =
        when (request.kind) {
          GalleryRequestKind.ADD_FAVORITES_URL,
          GalleryRequestKind.TORRENT_URL -> {
            val popup = request.kind == GalleryRequestKind.ADD_FAVORITES_URL
            ehUrl(
                path = listOf(if (popup) "gallerypopups.php" else "gallerytorrents.php"),
                host = host,
                parameters =
                  Parameters.build {
                    append("gid", "${request.gid}")
                    append("t", request.token)
                    if (popup) append("act", "addfav")
                  },
              )
              .buildString()
          }

          GalleryRequestKind.TRACKER_URL -> {
            ehUrl(
                listOfNotNull("${request.gid}", request.key, "announce"),
                "ehtracker.org",
              )
              .buildString()
          }

          GalleryRequestKind.TAG_DEFINITION_URL -> {
            "https://ehwiki.org/wiki/" + checkNotNull(fixture.tag).replace(' ', '_')
          }

          else -> {
            error("Unexpected fixture kind")
          }
        }
      assertEquals(fixture.url, legacy, fixture.name)
      assertEquals(
        plan(fixture),
        planGalleryRequest(request) { _, _ ->
          rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", wirePlan(fixture)))
        },
      )
    }
  }

  @Test
  fun invalidNavigationShapesAreRejectedBeforeAnyEffect() {
    for (fixture in fixtures().take(6)) {
      val valid = wirePlan(fixture)
      for (invalid in
        listOf(
          valid.copy(kind = GalleryRequestKind.RATE),
          valid.copy(url = "http://invalid/secret-key"),
          valid.copy(url = "https://evil.org/secret-key"),
          valid.copy(
            url =
              if (fixture.request.kind == GalleryRequestKind.TAG_DEFINITION_URL) "unexpected"
              else ""
          ),
          valid.copy(referer = "secret-key"),
          valid.copy(origin = "secret-key"),
          valid.copy(form = listOf(GalleryRequestFormField("secret-key", "value"))),
          valid.copy(body = "secret-key"),
          valid.copy(credentials = true),
        )) {
        var calls = 0
        val failure =
          assertFailsWith<IllegalArgumentException> {
            planGalleryRequest(fixture.request) { _, _ ->
              calls++
              rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", invalid))
            }
          }
        assertEquals(1, calls)
        assertFalse(failure.message.orEmpty().contains("secret-key"))
      }
    }
    val fixture = fixtures().first()
    val valid = plan(fixture)
    for (url in
      listOf(
        valid.url.replace("&act=addfav", "&act=bad"),
        valid.url.replace("&t=", "&wrong="),
        valid.url.replace("%2F", "%zz"),
        valid.url.replace("%2F", "/"),
        valid.url.replace("&act=addfav", "#fragment&act=addfav"),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(fixture.request) { _, _ ->
          rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", valid.copy(url = url)))
        }
      }
    }
    val tracker = fixtures().first { it.name == "tracker_empty" }
    assertFailsWith<IllegalArgumentException> {
      planGalleryRequest(tracker.request) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope(
            1,
            "galleryRequest",
            plan(tracker).copy(url = "https://ehtracker.org/-7/announce"),
          )
        )
      }
    }
  }

  @Test
  fun protocolBudgetsEnumsAndFailuresNeverFallbackOrExposePayloads() {
    val fixture = fixtures().first()
    val valid = plan(fixture)
    for (envelope in
      listOf(
        ApiEnvelope(2, "galleryRequest", valid),
        ApiEnvelope(1, "wrong", valid),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(fixture.request) { _, _ -> rustCbor.encodeToByteArray(envelope) }
      }
    }
    for (envelope in
      listOf(
        UncheckedEnvelope(1, "galleryRequest", null, null),
        UncheckedEnvelope(1, "galleryRequest", valid, RustError(RustErrorKind.ERROR, "fixed")),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(fixture.request) { _, _ -> rustCbor.encodeToByteArray(envelope) }
      }
    }
    assertFailsWith<RustFailureException> {
      planGalleryRequest(fixture.request) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope<GalleryRequestPlan>(
            1,
            "galleryRequest",
            error = RustError(RustErrorKind.ERROR, "fixed"),
          )
        )
      }
    }
    val unknown =
      bytes(
          "tracker_null",
          "response",
        )
        .toString(Charsets.ISO_8859_1)
        .replace("trackerUrl", "secret-key")
        .toByteArray(Charsets.ISO_8859_1)
    for (response in
      listOf(
        byteArrayOf(),
        byteArrayOf(0xff.toByte()),
        unknown,
        ByteArray(3 * 64 * 1024 + 257),
      )) {
      val failure =
        assertFailsWith<IllegalArgumentException> {
          planGalleryRequest(fixture.request) { _, _ -> response }
        }
      assertFalse(failure.message.orEmpty().contains("secret-key"))
    }
    for (request in
      listOf(
        fixture.request.copy(version = 2),
        fixture.request.copy(token = "😀".repeat(16_384) + "x"),
        fixture.request.copy(token = "", key = "x".repeat(65_537)),
        fixture.request.copy(token = "", wikiTagUtf16 = ByteArray(131_074)),
        fixture.request.copy(comment = "x".repeat(2 * 1024 * 1024)),
        fixture.request.copy(identities = List(251) { GalleryRequestIdentity(1, "x") }),
      )) {
      var calls = 0
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(request) { _, _ ->
          calls++
          error("must not call native")
        }
      }
      assertEquals(0, calls)
    }
    for (failure in
      listOf<Throwable>(
        IllegalStateException("native"),
        UnsatisfiedLinkError("JNI"),
      )) {
      var calls = 0
      try {
        planGalleryRequest(fixture.request) { _, _ ->
          calls++
          throw failure
        }
        error("Expected failure")
      } catch (actual: Throwable) {
        assertTrue(actual === failure)
      }
      assertEquals(1, calls)
    }
  }
}
