package com.hippo.ehviewer.client.parser

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentEntry
import com.hippo.ehviewer.bridge.TorrentListPage
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class TorrentParserTest {
  @Test
  fun defaultOffPathCallsOnlyTheLegacyParser() {
    var legacyCalled = false
    var rustCalled = false
    val expected = listOf(legacyTorrent())

    val result =
      TorrentParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        useRust = false,
        legacyParser = { expected.also { legacyCalled = true } },
        rustParser = {
          rustCalled = true
          error("Rust parser must remain disabled")
        },
      )

    assertEquals(expected, result)
    assertTrue(legacyCalled)
    assertFalse(rustCalled)
  }

  @Test
  fun enabledPathCallsOnlyRustAndUsesValidatedHash() {
    var legacyCalled = false
    val result =
      TorrentParser.parseWithAdapters(
        body = ByteBuffer.allocateDirect(8),
        useRust = true,
        legacyParser = {
          legacyCalled = true
          error("Legacy parser must not run")
        },
        rustParser = {
          ApiEnvelope(
            version = 1,
            module = "torrentList",
            payload = TorrentListPage(listOf(rustEntry())),
          )
        },
      )

    val torrent = result.single()
    assertEquals(HASH, torrent.infoHashForMagnet())
    assertEquals("", torrent.url)
    assertFalse(legacyCalled)
  }

  @Test
  fun rustFailureNeverFallsBackToLegacy() {
    var legacyCalled = false
    val failure = RustError(RustErrorKind.ERROR, "invalid torrent list")

    val error =
      assertFailsWith<RustFailureException> {
        TorrentParser.parseWithAdapters(
          body = ByteBuffer.allocateDirect(8),
          useRust = true,
          legacyParser = {
            legacyCalled = true
            emptyList()
          },
          rustParser = {
            ApiEnvelope(version = 1, module = "torrentList", error = failure)
          },
        )
      }

    assertEquals(failure, error.failure)
    assertFalse(legacyCalled)
  }

  @Test
  fun legacyHashFallbackRemainsByteCompatible() {
    val torrent = legacyTorrent()
    assertEquals(HASH, torrent.infoHashForMagnet())
    assertEquals("validated", torrent.copy(infoHash = "validated").infoHashForMagnet())
  }

  @Test
  fun legacyCborWithoutInfoHashUsesTheNewDefault() {
    val encoded =
      Cbor.encodeToByteArray(
        listOf(
          LegacyTorrent(
            outdated = false,
            posted = "2026-08-03",
            size = "1 MiB",
            seeds = 1,
            peers = 2,
            downloads = 3,
            uploader = "uploader",
            url = "/get/$HASH.torrent",
            name = "name",
          )
        )
      )

    val decoded = Cbor.decodeFromByteArray<TorrentResult>(encoded).single()

    assertEquals(null, decoded.infoHash)
    assertEquals(HASH, decoded.infoHashForMagnet())
  }

  private fun legacyTorrent() =
    Torrent(
      outdated = false,
      posted = "2026-08-03",
      size = "1 MiB",
      seeds = 1,
      peers = 2,
      downloads = 3,
      uploader = "uploader",
      url = "/get/$HASH.torrent",
      name = "name",
    )

  private fun rustEntry() =
    TorrentEntry(
      outdated = false,
      posted = "2026-08-03",
      size = "1 MiB",
      seeds = 1,
      peers = 2,
      downloads = 3,
      uploader = "uploader",
      infoHash = HASH,
      name = "name",
    )

  private companion object {
    const val HASH = "0123456789abcdef0123456789abcdef01234567"
  }

  @Serializable
  private data class LegacyTorrent(
    val outdated: Boolean,
    val posted: String,
    val size: String,
    val seeds: Int,
    val peers: Int,
    val downloads: Int,
    val uploader: String,
    val url: String,
    val name: String,
  )
}
