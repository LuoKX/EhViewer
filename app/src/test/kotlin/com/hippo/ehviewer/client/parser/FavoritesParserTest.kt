package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.encodeToByteArray

class FavoritesParserTest {
  @Test
  fun consumesTheCompleteBodyAndDecodesTheSharedRustSuccessGolden() {
    val html = "<textarea>Note &amp; more</textarea>"
    val note =
      FavoritesParser.parseNoteWithRust(directBody(html)) { buffer, size ->
        assertEquals(html, buffer.readUtf8(size))
        buffer.replaceWith(SUCCESS_CBOR_HEX.hexToBytes())
      }

    assertEquals("Note & more", note)
  }

  @Test
  fun rustErrorsAndContractDriftRemainInsideTheLegacyParseExceptionBoundary() {
    val rustFailure =
      assertFailsWith<ParseException> {
        FavoritesParser.parseNoteWithRust(directBody("missing")) { buffer, _ ->
          buffer.replaceWith(ERROR_CBOR_HEX.hexToBytes())
        }
      }
    assertEquals("Can't parse favorite note", rustFailure.message)
    assertIs<RustFailureException>(rustFailure.cause)

    for (envelope in
      listOf(
        ApiEnvelope(version = 2, module = "galleryFavoriteNote", payload = "note"),
        ApiEnvelope(version = 1, module = "wrong", payload = "note"),
      )) {
      val failure =
        assertFailsWith<ParseException> {
          FavoritesParser.parseNoteWithRust(directBody("body")) { buffer, _ ->
            buffer.replaceWith(Cbor.encodeToByteArray(envelope))
          }
        }
      assertEquals("Can't parse favorite note", failure.message)
    }
  }

  @Test
  fun nativeAndCborFailuresNeverSelectAKotlinParserFallback() {
    assertFailsWith<ParseException> {
      FavoritesParser.parseNoteWithRust(directBody("body")) { _, _ ->
        error("contained native failure")
      }
    }
    assertFailsWith<ParseException> {
      FavoritesParser.parseNoteWithRust(directBody("body")) { buffer, _ ->
        buffer.replaceWith(byteArrayOf(0xff.toByte()))
      }
    }
  }

  private fun directBody(value: String) =
    ByteBuffer.allocateDirect(512).apply {
      put(value.encodeToByteArray())
      flip()
    }

  private fun ByteBuffer.readUtf8(size: Int): String {
    val bytes = ByteArray(size)
    duplicate().apply {
      position(0)
      limit(size)
      get(bytes)
    }
    return bytes.decodeToString()
  }

  private fun ByteBuffer.replaceWith(bytes: ByteArray): Int {
    clear()
    put(bytes)
    position(0)
    return bytes.size
  }

  private fun String.hexToBytes() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

  private companion object {
    const val SUCCESS_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974654e6f7465677061796c6f61646b4e6f74652026206d6f7265656572726f72f6"
    const val ERROR_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657367616c6c6572794661766f726974654e6f7465677061796c6f6164f6656572726f72a2646b696e64656572726f72676d65737361676578214661766f72697465206e6f746520726573706f6e736520697320696e76616c6964"
  }
}
