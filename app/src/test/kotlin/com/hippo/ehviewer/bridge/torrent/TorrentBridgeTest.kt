package com.hippo.ehviewer.bridge.torrent

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.TorrentEntry
import com.hippo.ehviewer.bridge.TorrentListPage
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class TorrentBridgeTest {
  @Test
  fun decodesAndMapsTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<TorrentListPage>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val torrent = envelope.toLegacyTorrents().single()

    assertEquals(true, torrent.outdated)
    assertEquals("2026-08-03", torrent.posted)
    assertEquals("1.5 MiB", torrent.size)
    assertEquals(7, torrent.seeds)
    assertEquals(8, torrent.peers)
    assertEquals(9, torrent.downloads)
    assertEquals("uploader", torrent.uploader)
    assertEquals("Name", torrent.name)
    assertEquals(HASH, torrent.infoHash)
    assertEquals("", torrent.url)
  }

  @Test
  fun rejectsInvalidHashesContractsAndTypedFailures() {
    for (hash in listOf("short", HASH.uppercase(), "g".repeat(40))) {
      assertFailsWith<IllegalArgumentException> {
        successEnvelope(hash).toLegacyTorrents()
      }
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(HASH).copy(version = 2).toLegacyTorrents()
    }
    assertFailsWith<IllegalArgumentException> {
      successEnvelope(HASH).copy(module = "torrent").toLegacyTorrents()
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope<TorrentListPage>(version = 1, module = "torrentList").toLegacyTorrents()
    }

    val failure = RustError(RustErrorKind.ERROR, "invalid torrent list")
    val error =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<TorrentListPage>(version = 1, module = "torrentList", error = failure)
          .toLegacyTorrents()
      }
    assertEquals(failure, error.failure)
  }

  private fun successEnvelope(hash: String) =
    ApiEnvelope(
      version = 1,
      module = "torrentList",
      payload =
        TorrentListPage(
          listOf(
            TorrentEntry(
              outdated = false,
              posted = "2026-08-03",
              size = "1 MiB",
              seeds = 1,
              peers = 2,
              downloads = 3,
              uploader = "uploader",
              infoHash = hash,
              name = "name",
            )
          )
        ),
    )

  private companion object {
    const val HASH = "0123456789abcdef0123456789abcdef01234567"
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656b746f7272656e744c697374677061796c6f6164a1" +
        "68746f7272656e747381a9686f75746461746564f566706f737465646a323032362d30382d3033" +
        "6473697a6567312e35204d6942657365656473076570656572730869646f776e6c6f6164730968" +
        "75706c6f616465726875706c6f6164657268696e666f4861736878283031323334353637383961" +
        "6263646566303132333435363738396162636465663031323334353637646e616d65644e616d65" +
        "656572726f72f6"
  }
}
