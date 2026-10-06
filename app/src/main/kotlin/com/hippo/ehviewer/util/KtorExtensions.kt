package com.hippo.ehviewer.util

import io.ktor.client.call.body
import io.ktor.client.statement.HttpResponse
import io.ktor.http.HttpStatusCode
import io.ktor.http.isSuccess
import io.ktor.utils.io.ByteReadChannel
import io.ktor.utils.io.charsets.decode
import io.ktor.utils.io.readAvailable
import java.io.IOException
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.io.Source

suspend fun HttpResponse.bodyAsUtf8Text(): String {
  val decoder =
    Charsets.UTF_8.newDecoder().apply {
      onMalformedInput(CodingErrorAction.REPLACE)
      onUnmappableCharacter(CodingErrorAction.REPLACE)
    }
  val input = body<Source>()

  return decoder.decode(input)
}

/** Reads a complete response without spinning on a full buffer or accepting a failed channel. */
internal suspend fun ByteReadChannel.readToBuffer(buffer: ByteBuffer) {
  while (buffer.hasRemaining() && !isClosedForRead) {
    currentCoroutineContext().ensureActive()
    readAvailable(buffer)
  }
  currentCoroutineContext().ensureActive()
  closedCause?.let { throw it }
  // A full buffer is valid only when the response ends at exactly this boundary.
  if (awaitContent()) throw IOException("HTTP response exceeds buffer capacity")
  closedCause?.let { throw it }
}

fun HttpStatusCode.ensureSuccess() {
  if (!isSuccess()) throw Exception(toString())
}
