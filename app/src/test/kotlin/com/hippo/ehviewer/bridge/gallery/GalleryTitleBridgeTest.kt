package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryTitleBridgeTest {
  @Test
  fun requestAndResponseGoldensMatchRust() {
    val request = GalleryTitleRequest(1, "[Group] Title | Translation CHx 1-2")
    val envelope = ApiEnvelope(1, GALLERY_TITLE_MODULE, GalleryTitlePlan("Title"))

    assertEquals(REQUEST_CBOR_HEX, rustCbor.encodeToByteArray(request).toHex())
    assertEquals(
      envelope,
      rustCbor.decodeFromByteArray<ApiEnvelope<GalleryTitlePlan>>(RESPONSE_CBOR_HEX.decodeHex()),
    )
  }

  @Test
  fun fixedExamplesMatchTheFrozenKotlinRegexBehavior() {
    val cases =
      listOf(
        null to null,
        "" to null,
        " \t\n " to null,
        "Plain title" to "Plain title",
        "[Group] (Event) {Edition} ~Raw~ Title" to "Title",
        "Title CHx 1-23 [Digital] (Complete)" to "Title",
        "Title ch😀 1-2" to "Title",
        "Title ch\r 1-2" to "Title ch\r 1-2",
        "Romaji | English | Other" to "Romaji | English",
        "[Only] {metadata} ~here~" to null,
        "Title [Digital]\u2028" to "Title",
        "Title [Digital]\u2028\u2028" to "Title [Digital]",
        "\u2003 Unicode title \u2003" to "Unicode title",
      )

    cases.forEach { (input, expected) -> assertEquals(expected, frozenKotlinTitleCleanup(input)) }
  }

  @Test
  fun eachCompleteInputUsesExactlyOneNativeCall() {
    for ((input, expected) in
      listOf(
        "[Group] Title | Translation CHx 1-2" to "Title",
        null to null,
      )) {
      var calls = 0
      val actual =
        extractGalleryTitle(input) { requestBuffer, requestSize, responseBuffer ->
          calls += 1
          val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
          assertEquals(
            GalleryTitleRequest(GALLERY_TITLE_API_VERSION, input),
            rustCbor.decodeFromByteArray(requestBytes),
          )
          val response =
            rustCbor.encodeToByteArray(
              ApiEnvelope(1, GALLERY_TITLE_MODULE, GalleryTitlePlan(expected))
            )
          responseBuffer.put(response)
          response.size
        }

      assertEquals(expected, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun validatorRejectsMetadataFailureAndMissingPayload() {
    assertFailsWith<IllegalArgumentException> {
      extractWithEnvelope(ApiEnvelope(2, GALLERY_TITLE_MODULE, GalleryTitlePlan("Title")))
    }
    assertFailsWith<IllegalArgumentException> {
      extractWithEnvelope(ApiEnvelope(1, "other", GalleryTitlePlan("Title")))
    }
    assertFailsWith<RustFailureException> {
      extractWithEnvelope(
        ApiEnvelope(
          version = 1,
          module = GALLERY_TITLE_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
      )
    }
    assertFailsWith<IllegalArgumentException> {
      extractWithWireEnvelope(WireEnvelope(1, GALLERY_TITLE_MODULE))
    }
  }

  @Test
  fun nativeFailuresPropagateWithoutFallback() {
    assertFailsWith<LinkageError> {
      extractGalleryTitle("Title") { _, _, _ -> throw LinkageError("missing") }
    }
  }

  private fun extractWithEnvelope(envelope: ApiEnvelope<GalleryTitlePlan>) =
    extractGalleryTitle("Title") { _: ByteBuffer, _: Int, output: ByteBuffer ->
      val response = rustCbor.encodeToByteArray(envelope)
      output.put(response)
      response.size
    }

  private fun extractWithWireEnvelope(envelope: WireEnvelope<GalleryTitlePlan>) =
    extractGalleryTitle("Title") { _: ByteBuffer, _: Int, output: ByteBuffer ->
      val response = rustCbor.encodeToByteArray(envelope)
      output.put(response)
      response.size
    }

  private fun frozenKotlinTitleCleanup(fullTitle: String?): String? {
    var title = fullTitle ?: return null
    title = Regex("""^(?:\([^)]*\)|\[[^]]*]|\{[^}]*\}|~[^~]*~|\s+)*""").replaceFirst(title, "")
    title =
      Regex(
          """(?:\s+ch.[\s\d-]+)?(?:\([^)]*\)|\[[^]]*]|\{[^}]*\}|~[^~]*~|\s+)*$""",
          RegexOption.IGNORE_CASE,
        )
        .replaceFirst(title, "")
    return title.substringBeforeLast('|').trim().ifEmpty { null }
  }

  @Serializable
  private data class WireEnvelope<T>(
    val version: Int,
    val module: String,
    val payload: T? = null,
    val error: RustError? = null,
  )

  private companion object {
    const val REQUEST_CBOR_HEX =
      "a26776657273696f6e0165696e70757478235b47726f75705d205469746c65207c205472616e736c6174696f6e2043487820312d32"
    const val RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656c67616c6c6572795469746c65677061796c6f6164a1657469746c65655469746c65656572726f72f6"
  }
}

private fun ByteArray.toHex() = joinToString("") { byte -> "%02x".format(byte) }

private fun String.decodeHex() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
