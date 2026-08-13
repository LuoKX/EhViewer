package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryFavoritesPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toFavoritesResult
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import moe.tarsin.coroutines.runSuspendCatching

@Serializable
data class FavParserResult(
  val catArray: List<String>,
  val countArray: List<Int>,
  val galleryListResult: GalleryListResult,
) {
  val prev = galleryListResult.prev
  val next = galleryListResult.next
  val galleryInfoList = galleryListResult.galleryInfoList
}

object FavoritesParser {
  /** Selects exactly one response parser before the direct buffer is consumed. */
  fun parse(body: ByteBuffer): FavParserResult = runSuspendCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryFavoritesPage>>(body, ::parseFavV1).toFavoritesResult()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse favorites error", it)
    }

  /** Parses the current favorite note through the sole V1 Rust response grammar. */
  fun parseNote(body: ByteBuffer) = parseNoteWithRust(body)

  internal fun parseNoteWithRust(
    body: ByteBuffer,
    nativeCall: (ByteBuffer, Int) -> Int = ::parseFavoriteNoteV1,
  ): String = runSuspendCatching {
    unmarshalParsingAs<ApiEnvelope<String>>(body, nativeCall).requireFavoriteNotePayload()
  }
    .getOrElse { throw ParseException("Can't parse favorite note", it) }
}

/** Invokes the V1 `eh_net::gallery::favorites` response parser. */
private external fun parseFavV1(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::gallery::favorite_note` response parser. */
private external fun parseFavoriteNoteV1(body: ByteBuffer, limit: Int = body.limit()): Int

private fun ApiEnvelope<String>.requireFavoriteNotePayload(): String {
  require(version == FAVORITE_NOTE_API_VERSION) { "Unsupported favorite-note API version" }
  require(module == FAVORITE_NOTE_MODULE) { "Unexpected favorite-note Rust module" }
  error?.let { throw RustFailureException(it) }
  return checkNotNull(payload) { "Favorite-note Rust response has no payload" }
}

private const val FAVORITE_NOTE_API_VERSION = 1
private const val FAVORITE_NOTE_MODULE = "galleryFavoriteNote"
