package com.hippo.ehviewer.bridge.download

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class DownloadCatalogBridgeTest {
  @Test
  fun sortBridgeAcceptsOneCompletePermutation() {
    val request = sortRequest()
    val expected = DownloadSortPlan(listOf(2, 1))
    var calls = 0
    val actual =
      sortDownloadCatalog(request) { input: ByteBuffer, size: Int, output: ByteBuffer ->
        calls += 1
        val bytes = ByteArray(size).also { input.get(it) }
        assertEquals(request, rustCbor.decodeFromByteArray<DownloadSortRequest>(bytes))
        respond(output, ApiEnvelope(1, DOWNLOAD_CATALOG_MODULE, expected))
      }

    assertEquals(expected, actual)
    assertEquals(1, calls)
  }

  @Test
  fun filterBridgeAcceptsOnlyStableDistinctSubsets() {
    val request = filterRequest()
    val expected = DownloadFilterPlan(listOf(1, 2))
    assertEquals(
      expected,
      filterDownloadCatalog(request) { _, _, output ->
        respond(output, ApiEnvelope(1, DOWNLOAD_CATALOG_MODULE, expected))
      },
    )
    assertFailsWith<IllegalArgumentException> {
      filterDownloadCatalog(request) { _, _, output ->
        respond(output, ApiEnvelope(1, DOWNLOAD_CATALOG_MODULE, DownloadFilterPlan(listOf(2, 1))))
      }
    }
    assertFailsWith<IllegalArgumentException> {
      filterDownloadCatalog(request) { _, _, output ->
        respond(output, ApiEnvelope(1, DOWNLOAD_CATALOG_MODULE, DownloadFilterPlan(listOf(1, 1))))
      }
    }
  }

  @Test
  fun selectedNativeFailuresPropagateWithoutAPlan() {
    assertFailsWith<LinkageError> {
      sortDownloadCatalog(sortRequest()) { _, _, _ -> throw LinkageError("missing") }
    }
  }

  private fun sortRequest() =
    DownloadSortRequest(
      version = DOWNLOAD_CATALOG_API_VERSION,
      entries = entries,
      field = DownloadCatalogSortField.TIME,
      order = DownloadCatalogSortOrder.DESCENDING,
      groupByDownloadLabel = false,
      preferJapaneseTitle = false,
    )

  private fun filterRequest() =
    DownloadFilterRequest(
      version = DOWNLOAD_CATALOG_API_VERSION,
      entries = entries,
      mode = DownloadCatalogFilterMode.CUSTOM,
      label = "",
      state = -1,
      keyword = "",
    )

  private inline fun <reified T> respond(output: ByteBuffer, envelope: T): Int {
    val response = rustCbor.encodeToByteArray(envelope)
    output.put(response)
    return response.size
  }

  private companion object {
    val entries =
      listOf(
        DownloadCatalogEntry(1, "a", null, 1, 1, null, 0, emptyList(), emptyList()),
        DownloadCatalogEntry(2, "b", null, 2, 2, "saved", 3, listOf("artist"), listOf("tag")),
      )
  }
}
