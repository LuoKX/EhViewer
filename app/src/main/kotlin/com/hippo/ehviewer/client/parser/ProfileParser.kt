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

import com.hippo.ehviewer.bridge.AccountProfile
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.auth.requireAuthPayload
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable
import moe.tarsin.coroutines.runSuspendCatching

object ProfileParser {
  fun parse(body: ByteBuffer) =
    parseOrThrow("Failed to parse profile") {
      val profile =
        unmarshalParsingAs<ApiEnvelope<AccountProfile>>(body, ::parseProfileV1).requireAuthPayload()
      Result(profile.displayName, profile.avatar)
    }

  fun parseProfileUrl(body: ByteBuffer) =
    parseOrThrow("Failed to parse profile URL") {
      unmarshalParsingAs<ApiEnvelope<String>>(body, ::parseProfileUrlV1).requireAuthPayload()
    }

  @Serializable data class Result(val displayName: String, val avatar: String?)

  /** Invokes the V1 `eh_net::auth::profile` account parser. */
  private external fun parseProfileV1(body: ByteBuffer, size: Int = body.limit()): Int

  /** Invokes the V1 `eh_net::auth::profile` URL parser. */
  private external fun parseProfileUrlV1(body: ByteBuffer, size: Int = body.limit()): Int

  private inline fun <T> parseOrThrow(message: String, block: () -> T): T =
    runSuspendCatching(block).getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException(message, it)
    }
}
