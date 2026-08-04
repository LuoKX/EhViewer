package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.TorrentListPage
import com.hippo.ehviewer.bridge.torrent.toLegacyTorrents
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable

object TorrentParser {
  fun parse(body: ByteBuffer) = runCatching {
    parseWithAdapters(
      body = body,
      useRust = Settings.useRustCore.value,
      legacyParser = { unmarshalParsingAs<TorrentResult>(it, ::parseTorrent) },
      rustParser = {
        unmarshalParsingAs<ApiEnvelope<TorrentListPage>>(it, ::parseTorrentV1)
      },
    )
  }
    .getOrElse {
      throw ParseException("Can't parse torrent list", it)
    }

  internal fun parseWithAdapters(
    body: ByteBuffer,
    useRust: Boolean,
    legacyParser: (ByteBuffer) -> TorrentResult,
    rustParser: (ByteBuffer) -> ApiEnvelope<TorrentListPage>,
  ): TorrentResult =
    if (useRust) {
      rustParser(body).toLegacyTorrents()
    } else {
      legacyParser(body)
    }
}

@Serializable
data class Torrent(
  val outdated: Boolean,
  val posted: String,
  val size: String,
  val seeds: Int,
  val peers: Int,
  val downloads: Int,
  val uploader: String,
  val url: String,
  val name: String,
  val infoHash: String? = null,
)

fun Torrent.format() = "↑$seeds ↓$peers ✓$downloads"

/** Returns the V1 validated hash, or the frozen legacy URL-derived value. */
internal fun Torrent.infoHashForMagnet() = infoHash ?: url.dropLast(8).takeLast(40)

typealias TorrentResult = List<Torrent>

private external fun parseTorrent(body: ByteBuffer, size: Int = body.limit()): Int

/** Invokes the V1 `eh_net::torrent` parser without exposing its source URL. */
private external fun parseTorrentV1(body: ByteBuffer, size: Int = body.limit()): Int
