package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.TorrentListPage
import com.hippo.ehviewer.bridge.torrent.toLegacyTorrents
import com.hippo.ehviewer.client.exception.ParseException
import java.nio.ByteBuffer
import kotlinx.serialization.Serializable

object TorrentParser {
  fun parse(body: ByteBuffer) = runCatching {
    unmarshalParsingAs<ApiEnvelope<TorrentListPage>>(body, ::parseTorrentV1).toLegacyTorrents()
  }
    .getOrElse {
      throw ParseException("Can't parse torrent list", it)
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

/** Returns the validated hash supplied by the Rust parser. */
internal fun Torrent.infoHashForMagnet() =
  requireNotNull(infoHash) { "Torrent info hash is missing" }

typealias TorrentResult = List<Torrent>

/** Invokes the V1 `eh_net::torrent` parser without exposing its source URL. */
private external fun parseTorrentV1(body: ByteBuffer, size: Int = body.limit()): Int
