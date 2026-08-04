package com.hippo.ehviewer.bridge.torrent

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentListPage
import com.hippo.ehviewer.client.parser.Torrent

/** Validates and adapts the V1 `eh_net::torrent` response. */
internal fun ApiEnvelope<TorrentListPage>.toLegacyTorrents(): List<Torrent> {
  require(version == TORRENT_API_VERSION) { "Unsupported Torrent API version: $version" }
  require(module == TORRENT_MODULE) { "Unexpected Rust module: $module" }
  error?.let { throw RustFailureException(it) }

  return checkNotNull(payload) { "Rust Torrent response has no payload" }
    .torrents
    .map { entry ->
      require(INFO_HASH.matches(entry.infoHash)) {
        "Rust Torrent response has an invalid info hash"
      }
      Torrent(
        outdated = entry.outdated,
        posted = entry.posted,
        size = entry.size,
        seeds = entry.seeds,
        peers = entry.peers,
        downloads = entry.downloads,
        uploader = entry.uploader,
        url = "",
        name = entry.name,
        infoHash = entry.infoHash,
      )
    }
}

private const val TORRENT_API_VERSION = 1
private const val TORRENT_MODULE = "torrentList"
private val INFO_HASH = Regex("[0-9a-f]{40}")
