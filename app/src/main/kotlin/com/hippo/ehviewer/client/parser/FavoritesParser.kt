package com.hippo.ehviewer.client.parser

import com.ehviewer.core.util.unescapeXml
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
  private val NoteRegex = Regex("<textarea[^>]*>([^<]*)</textarea>")

  /** Selects exactly one response parser before the direct buffer is consumed. */
  fun parse(body: ByteBuffer): FavParserResult = runSuspendCatching {
    unmarshalParsingAs<ApiEnvelope<GalleryFavoritesPage>>(body, ::parseFavV1).toFavoritesResult()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse favorites error", it)
    }

  fun parseNote(body: String) =
    NoteRegex.find(body)?.run { groupValues[1].unescapeXml() }
      ?: throw ParseException("Can't parse favorite note")
}

/** Invokes the V1 `eh_net::gallery::favorites` response parser. */
private external fun parseFavV1(body: ByteBuffer, limit: Int = body.limit()): Int
