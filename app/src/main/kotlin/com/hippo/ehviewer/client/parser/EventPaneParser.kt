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
import com.hippo.ehviewer.bridge.NewsPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.news.toEventHtml
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import moe.tarsin.coroutines.runSuspendCatching

object EventPaneParser {
  fun parse(body: ByteBuffer): String? = runSuspendCatching {
    unmarshalParsingAs<ApiEnvelope<NewsPage>>(body, ::nativeParseNewsV1).toEventHtml()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Failed to parse news", it)
    }

  /** Invokes the V1 `eh_net::news` event-pane parser. */
  private external fun nativeParseNewsV1(body: ByteBuffer, size: Int = body.limit()): Int
}
