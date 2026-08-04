package com.hippo.ehviewer.client.parser

import com.ehviewer.core.util.unescapeXml
import com.hippo.ehviewer.Settings
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
    parseWithAdapters(
      useRust = Settings.useRustCore.value,
      legacyParser = { unmarshalParsingAs<FavParserResult>(body, ::parseFav) },
      rustParser = {
        unmarshalParsingAs<ApiEnvelope<GalleryFavoritesPage>>(body, ::parseFavV1)
          .toFavoritesResult()
      },
    )
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse favorites error", it)
    }

  /** Chooses one complete adapter without replaying a GET or completed mutation response. */
  internal fun parseWithAdapters(
    useRust: Boolean,
    legacyParser: () -> FavParserResult,
    rustParser: () -> FavParserResult,
  ): FavParserResult = if (useRust) rustParser() else legacyParser()

  fun parseNote(body: String) =
    NoteRegex.find(body)?.run { groupValues[1].unescapeXml() }
      ?: throw ParseException("Can't parse favorite note")
}

/** Invokes the frozen, unversioned Favorites rollback parser. */
private external fun parseFav(body: ByteBuffer, limit: Int = body.limit()): Int

/** Invokes the V1 `eh_net::gallery::favorites` response parser. */
private external fun parseFavV1(body: ByteBuffer, limit: Int = body.limit()): Int
