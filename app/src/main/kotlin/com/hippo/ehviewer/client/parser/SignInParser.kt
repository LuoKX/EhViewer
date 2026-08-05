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

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.SignInIdentity
import com.hippo.ehviewer.bridge.auth.requireAuthPayload
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import moe.tarsin.coroutines.runSuspendCatching

object SignInParser {
  fun parseV1(body: ByteBuffer): String = runSuspendCatching {
    unmarshalParsingAs<ApiEnvelope<SignInIdentity>>(body, ::parseSignInV1)
      .requireAuthPayload()
      .username
  }
    .getOrElse {
      if (it is RustFailureException) throw it
      throw ParseException("Can't parse V1 sign in response", it)
    }

  /** Invokes the V1 `eh_net::auth::sign_in` parser without transferring credentials to Rust. */
  private external fun parseSignInV1(body: ByteBuffer, size: Int = body.limit()): Int
}
