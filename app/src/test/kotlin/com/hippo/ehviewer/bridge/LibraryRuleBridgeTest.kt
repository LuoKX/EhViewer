package com.hippo.ehviewer.bridge

import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.encodeToByteArray

class LibraryRuleBridgeTest {
  @Test
  fun legacyQueryAndQuickSearchRulesPreserveExactFields() {
    val query = DatabaseSearchQueryRequest.create("artist name")
    assertEquals("*artist name*", resolveDatabaseSearchQuery(query, CoreBackend.LEGACY))

    val snapshot = QuickSearchSnapshot(0, 1, "query", 2, 3, 4, 5)
    val request = QuickSearchMatchRequest(1, snapshot, snapshot.copy())
    assertEquals(true, resolveQuickSearchMatch(request, CoreBackend.LEGACY))
    assertEquals(
      false,
      resolveQuickSearchMatch(request.copy(saved = snapshot.copy(pageTo = 6)), CoreBackend.LEGACY),
    )
  }

  @Test
  fun legacyFavoritePlannerCoversCloudAndLocalTransitions() {
    assertEquals(
      LocalFavoriteMutationPlan(LocalFavoriteAction.REMOVE_CLOUD, -1, null, false),
      planLegacyLocalFavoriteMutation(LocalFavoriteMutationRequest(1, -2, 4, true)),
    )
    assertEquals(
      LocalFavoriteMutationPlan(LocalFavoriteAction.ADD_LOCAL, 4, null, true),
      planLegacyLocalFavoriteMutation(LocalFavoriteMutationRequest(1, -1, 4, false)),
    )
    assertEquals(
      LocalFavoriteMutationPlan(LocalFavoriteAction.SET_CLOUD, 7, 7, true),
      planLegacyLocalFavoriteMutation(LocalFavoriteMutationRequest(1, 7, -2, false)),
    )
  }

  @Test
  fun selectedRustFailurePropagatesBeforeFavoriteEffects() {
    val request = LocalFavoriteMutationRequest(1, -1, -2, false)
    assertFailsWith<LinkageError> {
      resolveLocalFavoriteMutationWithNative(request) { _, _, _ -> throw LinkageError("missing") }
    }
  }

  private fun resolveLocalFavoriteMutationWithNative(
    request: LocalFavoriteMutationRequest,
    nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
  ): LocalFavoriteMutationPlan {
    val envelopeCall = nativeCall
    return planLocalFavoriteForTest(request, envelopeCall)
  }

  private fun planLocalFavoriteForTest(
    request: LocalFavoriteMutationRequest,
    nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
  ): LocalFavoriteMutationPlan {
    val response = ByteBuffer.allocateDirect(4096)
    val input = rustCbor.encodeToByteArray(request)
    val inputBuffer =
      ByteBuffer.allocateDirect(input.size).apply {
        put(input)
        flip()
      }
    nativeCall(inputBuffer, input.size, response)
    error("Native call unexpectedly returned")
  }
}
