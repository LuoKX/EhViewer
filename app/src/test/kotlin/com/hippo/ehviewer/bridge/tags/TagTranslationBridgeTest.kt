package com.hippo.ehviewer.bridge.tags

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.rustCbor
import java.io.File
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray
import org.junit.Assert.*
import org.junit.Test

class TagTranslationBridgeTest {
  private val request = TagSuggestionRequest(1, "f:aa", true, 15)

  private fun fixture(name: String): ByteArray =
    File("src/main/rust/crates/net/test_data/$name.cbor.hex")
      .readText()
      .trim()
      .chunked(2)
      .map { it.toInt(16).toByte() }
      .toByteArray()

  @Test
  fun rustGoldensPinRequestResponseAndSingleNativeInvocation() {
    assertArrayEquals(fixture("tag_translation_request"), rustCbor.encodeToByteArray(request))
    var calls = 0
    var closes = 0
    val api =
      TagIndexNativeApi(
        open = { version, descriptor, remote ->
          assertEquals(1, version)
          assertEquals(88, descriptor)
          assertFalse(remote)
          7
        },
        suggest = { handle, buffer, size ->
          calls++
          assertEquals(7L, handle)
          assertTrue(buffer.isDirect)
          val bytes = ByteArray(size).also(buffer::get)
          assertEquals(request, rustCbor.decodeFromByteArray<TagSuggestionRequest>(bytes))
          fixture("tag_translation_response")
        },
        translate = { handle, prefix, tag ->
          assertEquals(7L, handle)
          assertEquals("f", prefix)
          assertEquals("aa", tag)
          "同"
        },
        close = {
          assertEquals(7L, it)
          closes++
        },
      )
    val index = TagTranslationIndex.open(88, false, api)
    assertEquals(listOf(TagSuggestion("f:aa", "同", 2f)), index.suggest(request))
    assertEquals("同", index.translate("f", "aa"))
    index.close()
    index.close()
    assertEquals(1, calls)
    assertEquals(1, closes)
    assertThrows(IllegalStateException::class.java) { index.suggest(request) }
  }

  @Test
  fun nativeFailuresNeverFallbackOrReplay() {
    for (failure in
      listOf<Throwable>(
        IllegalStateException("native failure"),
        UnsatisfiedLinkError("missing JNI"),
      )) {
      var calls = 0
      val api =
        TagIndexNativeApi(
          open = { _, _, _ -> 1 },
          suggest = { _, _, _ ->
            calls++
            throw failure
          },
          close = {},
        )
      TagTranslationIndex.open(0, true, api).use { index ->
        assertSame(failure, assertThrows(failure.javaClass) { index.suggest(request) })
      }
      assertEquals(1, calls)
    }
    assertThrows(IllegalStateException::class.java) {
      TagTranslationIndex.open(0, true, TagIndexNativeApi(open = { _, _, _ -> 0 }))
    }
  }

  @Test
  fun malformedContractsRowsAndRequestVersionsFailClosed() {
    val valid = ApiEnvelope(1, "tagTranslation", listOf(TagSuggestion("f:aa", "同", 2f)), null)
    val responses =
      listOf(
        byteArrayOf(),
        byteArrayOf(0),
        rustCbor.encodeToByteArray(valid.copy(version = 2)),
        rustCbor.encodeToByteArray(valid.copy(module = "wrong")),
        rustCbor.encodeToByteArray(valid.copy(payload = listOf(TagSuggestion("", null, 1f)))),
        rustCbor.encodeToByteArray(
          valid.copy(payload = listOf(TagSuggestion("f:aa", null, Float.NaN)))
        ),
        rustCbor.encodeToByteArray(
          valid.copy(payload = null, error = RustError(RustErrorKind.ERROR, "fixed failure"))
        ),
      )
    responses.forEach { response ->
      val api =
        TagIndexNativeApi(open = { _, _, _ -> 1 }, suggest = { _, _, _ -> response }, close = {})
      TagTranslationIndex.open(0, true, api).use { index ->
        assertThrows(Exception::class.java) { index.suggest(request) }
        assertThrows(IllegalArgumentException::class.java) {
          index.suggest(request.copy(version = 2))
        }
        assertThrows(IllegalArgumentException::class.java) {
          index.suggest(request.copy(limit = 51))
        }
      }
    }
    val typed =
      rustCbor.encodeToByteArray(
        ApiEnvelope<List<TagSuggestion>>(
          1,
          "tagTranslation",
          null,
          RustError(RustErrorKind.ERROR, "fixed failure"),
        )
      )
    TagTranslationIndex.open(
        0,
        true,
        TagIndexNativeApi(open = { _, _, _ -> 1 }, suggest = { _, _, _ -> typed }, close = {}),
      )
      .use {
        assertThrows(RustFailureException::class.java) { it.suggest(request) }
      }
  }
}
