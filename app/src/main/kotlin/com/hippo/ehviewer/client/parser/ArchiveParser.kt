/*
 * Copyright 2016 Hippo Seven
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package com.hippo.ehviewer.client.parser

import arrow.core.Either.Companion.catch
import arrow.core.getOrElse
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveDownloadTarget
import com.hippo.ehviewer.bridge.ArchiveListPage
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.gallery.toArchiveResult
import com.hippo.ehviewer.bridge.gallery.toArchiveUrl
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable

object ArchiveParser {
  fun parse(body: ByteBuffer, maybeFunds: Funds?) = catch {
    unmarshalParsingAs<ApiEnvelope<ArchiveListPage>>(body) { buffer, size ->
        parseArchivesV1(buffer, size, maybeFunds == null)
      }
      .toArchiveResult(maybeFunds)
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Can't parse archive list", it)
    }

  fun parseArchiveUrl(body: ByteBuffer) = catch {
    unmarshalParsingAs<ApiEnvelope<ArchiveDownloadTarget>>(body, ::parseArchiveUrlV1).toArchiveUrl()
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Can't parse archive url", it)
    }

  @Serializable data class Result(val archiveList: List<Archive>, val funds: Funds)
}

@Serializable
class Archive(
  val res: String,
  val name: String,
  val size: String,
  val cost: String,
  val isHath: Boolean,
)

/** Invokes the V1 `eh_net::gallery::archive` list parser. */
private external fun parseArchivesV1(body: ByteBuffer, size: Int, includeFunds: Boolean): Int

/** Invokes the V1 `eh_net::gallery::archive` final-target parser. */
private external fun parseArchiveUrlV1(body: ByteBuffer, size: Int = body.limit()): Int
