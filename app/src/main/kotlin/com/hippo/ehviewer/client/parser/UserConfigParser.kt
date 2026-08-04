package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.FavoriteCategories
import com.hippo.ehviewer.bridge.config.toFavoriteCategoryArray
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer

object UserConfigParser {
  private external fun parseFavCat(body: ByteBuffer, size: Int = body.limit()): Int

  fun parse(body: ByteBuffer) = runCatching {
    Settings.favCat =
      parseWithAdapters(
        body = body,
        useRust = Settings.useRustCore.value,
        legacyParser = { unmarshalParsingAs<Array<String>>(it, ::parseFavCat) },
        rustParser = {
          unmarshalParsingAs<ApiEnvelope<FavoriteCategories>>(it, ::parseFavCatV1)
        },
      )
  }
    .onFailure {
      throw ParseException("Can't parse user config", it)
    }

  internal fun parseWithAdapters(
    body: ByteBuffer,
    useRust: Boolean,
    legacyParser: (ByteBuffer) -> Array<String>,
    rustParser: (ByteBuffer) -> ApiEnvelope<FavoriteCategories>,
  ): Array<String> =
    if (useRust) {
      rustParser(body).toFavoriteCategoryArray()
    } else {
      legacyParser(body)
    }

  /** Invokes the V1 `eh_net::user_config` favorite-category parser. */
  private external fun parseFavCatV1(body: ByteBuffer, size: Int = body.limit()): Int
}
