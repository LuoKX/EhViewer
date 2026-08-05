package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class LibraryRuleBridgeTest {
  @Test
  fun databaseAndQuickSearchRequestsUseRustPlans() {
    val databaseRequest = DatabaseSearchQueryRequest.create("artist name")
    assertEquals(
      "*artist name*",
      resolveDatabaseSearchQuery(databaseRequest) { input, size, output ->
        assertEquals(databaseRequest, input.decodeRequest<DatabaseSearchQueryRequest>(size))
        output.writeEnvelope(DatabaseSearchQueryPlan("*artist name*"))
      },
    )

    val snapshot = QuickSearchSnapshot(0, 1, "query", 2, 3, 4, 5)
    val quickSearchRequest = QuickSearchMatchRequest(1, snapshot, snapshot.copy())
    assertEquals(
      true,
      resolveQuickSearchMatch(quickSearchRequest) { input, size, output ->
        assertEquals(quickSearchRequest, input.decodeRequest<QuickSearchMatchRequest>(size))
        output.writeEnvelope(QuickSearchMatchPlan(true))
      },
    )
  }

  @Test
  fun localFavoriteUsesRustPlanAndValidatesItsShape() {
    val request = LocalFavoriteMutationRequest(1, 7, -2, false)
    val expected = LocalFavoriteMutationPlan(LocalFavoriteAction.SET_CLOUD, 7, 7, true)
    assertEquals(
      expected,
      resolveLocalFavoriteMutation(request) { input, size, output ->
        assertEquals(request, input.decodeRequest<LocalFavoriteMutationRequest>(size))
        output.writeEnvelope(expected)
      },
    )

    assertFailsWith<IllegalArgumentException> {
      resolveLocalFavoriteMutation(request) { _, _, output ->
        output.writeEnvelope(expected.copy(cloudSlot = null))
      }
    }
  }

  @Test
  fun nativeFailurePropagatesWithoutFallback() {
    assertFailsWith<LinkageError> {
      resolveDatabaseSearchQuery(DatabaseSearchQueryRequest.create("query")) { _, _, _ ->
        throw LinkageError("missing")
      }
    }
  }

  private inline fun <reified T> ByteBuffer.decodeRequest(size: Int): T =
    rustCbor.decodeFromByteArray(ByteArray(size).also(::get))

  private inline fun <reified T> ByteBuffer.writeEnvelope(payload: T): Int {
    val response =
      rustCbor.encodeToByteArray(
        ApiEnvelope(
          version = LIBRARY_RULE_API_VERSION,
          module = LIBRARY_RULE_MODULE,
          payload = payload,
        )
      )
    put(response)
    return response.size
  }
}
