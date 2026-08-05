package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.FavoriteCategories
import com.hippo.ehviewer.bridge.config.toFavoriteCategoryArray
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer

object UserConfigParser {
  fun parse(body: ByteBuffer) = runCatching {
    Settings.favCat =
      unmarshalParsingAs<ApiEnvelope<FavoriteCategories>>(body, ::parseFavCatV1)
        .toFavoriteCategoryArray()
  }
    .onFailure {
      throw ParseException("Can't parse user config", it)
    }

  /** Invokes the V1 `eh_net::user_config` favorite-category parser. */
  private external fun parseFavCatV1(body: ByteBuffer, size: Int = body.limit()): Int
}
