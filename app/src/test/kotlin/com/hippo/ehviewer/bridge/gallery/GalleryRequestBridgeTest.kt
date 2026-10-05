package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.client.ehUrl
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.http.Parameters
import io.ktor.http.content.TextContent
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryRequestBridgeTest {
  @Test
  fun exactRustRequestAndResponseGoldensAreShared() {
    fun fixture(name: String) =
      File("src/main/rust/crates/net/test_data/gallery_request_$name.cbor.hex")
        .readText()
        .trim()
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
    val request =
      GalleryRequest(
        1,
        GalleryUrlSite.PUBLIC,
        GalleryRequestKind.RATE,
        gid = 7,
        token = "abc",
        rating = 2.01f,
      )
    assertEquals(fixture("request").toList(), rustCbor.encodeToByteArray(request).toList())
    val result = planGalleryRequest(request) { _, _ -> fixture("response") }
    assertEquals("""{"method":"rategallery","gid":7,"token":"abc","rating":5}""", result.body)
  }

  @Test
  fun uriAlphabetMatchesFrozenKtorBuilders() {
    val token = "a b/?#[];%"
    val mpv = ehUrl(listOf("mpv", "-7", token), "exhentai.org", Parameters.Empty).buildString()
    val page =
      ehUrl(
          listOf("s", "abc", "-7-${Int.MIN_VALUE}"),
          "e-hentai.org",
          Parameters.build { append("nl", "a b+&*~") },
        )
        .buildString()
    assertEquals("https://exhentai.org/mpv/-7/a%20b%2F%3F%23%5B%5D;%25", mpv)
    assertEquals("https://e-hentai.org/s/abc/-7--2147483648?nl=a+b%2B%26%2A~", page)
  }

  @Test
  fun fullRequestUsesOneNativeCallAndCredentialsStayOnAndroid() {
    val request =
      GalleryRequest(
        1,
        GalleryUrlSite.PUBLIC,
        GalleryRequestKind.RATE,
        gid = 7,
        token = "abc",
        rating = 2.01f,
      )
    val plan =
      GalleryRequestPlan(
        GalleryRequestKind.RATE,
        "https://api.e-hentai.org/api.php",
        "https://e-hentai.org/g/7/abc",
        "https://e-hentai.org",
        emptyList(),
        """{"method":"rategallery","gid":7,"token":"abc","rating":5}""",
        true,
      )
    var calls = 0
    val actual =
      planGalleryRequest(request) { buffer, size ->
        calls++
        assertTrue(buffer.isDirect)
        val bytes = ByteArray(size).also(buffer::get)
        assertEquals(request, rustCbor.decodeFromByteArray<GalleryRequest>(bytes))
        assertFalse(bytes.toString(Charsets.UTF_8).contains("apikey"))
        rustCbor.encodeToByteArray(ApiEnvelope(1, "galleryRequest", plan))
      }
    assertEquals(plan, actual)
    assertEquals(1, calls)
    val builder = HttpRequestBuilder().apply { applyGalleryRequestPlan(plan, 99, "test-api-key") }
    val body = (builder.body as TextContent).text
    assertEquals(
      """{"method":"rategallery","apiuid":99,"apikey":"test-api-key","gid":7,"token":"abc","rating":5}""",
      body,
    )
    assertFailsWith<IllegalArgumentException> { HttpRequestBuilder().applyGalleryRequestPlan(plan) }
  }

  @Test
  fun malformedEnvelopesAndNativeFailuresNeverFallback() {
    val request = GalleryRequest(1, GalleryUrlSite.PUBLIC, GalleryRequestKind.PAGE_URL)
    val plan =
      GalleryRequestPlan(
        GalleryRequestKind.PAGE_URL,
        "https://e-hentai.org/s/x/1-1",
        null,
        null,
        emptyList(),
        null,
        false,
      )
    for (envelope in
      listOf(
        ApiEnvelope(2, "galleryRequest", plan),
        ApiEnvelope(1, "other", plan),
        ApiEnvelope(1, "galleryRequest", plan.copy(kind = GalleryRequestKind.RATE)),
        ApiEnvelope(1, "galleryRequest", plan.copy(url = "")),
      )) {
      assertFailsWith<IllegalArgumentException> {
        planGalleryRequest(request) { _, _ -> rustCbor.encodeToByteArray(envelope) }
      }
    }
    assertFailsWith<RustFailureException> {
      planGalleryRequest(request) { _, _ ->
        rustCbor.encodeToByteArray(
          ApiEnvelope<GalleryRequestPlan>(
            1,
            "galleryRequest",
            error = RustError(RustErrorKind.ERROR, "fixed"),
          )
        )
      }
    }
    for (failure in
      listOf<Throwable>(IllegalStateException("native"), UnsatisfiedLinkError("JNI"))) {
      var calls = 0
      try {
        planGalleryRequest(request) { _, _ ->
          calls++
          throw failure
        }
        error("failure expected")
      } catch (actual: Throwable) {
        assertTrue(actual === failure)
      }
      assertEquals(1, calls)
    }
  }
}
