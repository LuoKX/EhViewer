/*
 * Copyright 2022 Tarsin Norbin
 *
 * This file is part of EhViewer
 *
 * EhViewer is free software: you can redistribute it and/or
 * modify it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * EhViewer is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
 * or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with EhViewer.
 * If not, see <https://www.gnu.org/licenses/>.
 */
package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.MpvTokens
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.reader.toPTokenList
import com.hippo.ehviewer.client.exception.ParseException
import com.hippo.ehviewer.client.parseAs
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

object GalleryMultiPageViewerPTokenParser {
  private const val IMAGE_LIST_STRING = "var imagelist = "

  fun parse(body: String): List<String> = runCatching {
    val index = body.indexOf(IMAGE_LIST_STRING)
    val imageList = body.substring(index + IMAGE_LIST_STRING.length, body.indexOf(";", index))
    imageList.parseAs<List<Item>>().map(Item::token)
  }
    .getOrElse {
      throw ParseException("Parse pToken from MPV error", it)
    }

  fun parseV1(body: ByteBuffer): List<String> = runCatching {
    unmarshalParsingAs<ApiEnvelope<MpvTokens>>(body, ::nativeParseMpvTokensV1).toPTokenList()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Parse V1 pToken from MPV error", it)
    }

  internal suspend fun parseWithAdapters(
    useRust: Boolean,
    legacyParser: suspend () -> List<String>,
    rustParser: suspend () -> List<String>,
  ): List<String> = if (useRust) rustParser() else legacyParser()

  @Serializable data class Item(@SerialName("k") val token: String)

  /** Invokes the V1 `eh_net::reader::mpv` parser without moving HTTP or cache state to Rust. */
  private external fun nativeParseMpvTokensV1(
    body: ByteBuffer,
    size: Int = body.limit(),
  ): Int
}
