package com.hippo.ehviewer.bridge

import com.ehviewer.core.database.model.QuickSearch
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToByteArray

@Serializable
internal data class DatabaseSearchQueryRequest(val version: Int, val keyword: String) {
  companion object {
    fun create(keyword: String) = DatabaseSearchQueryRequest(LIBRARY_RULE_API_VERSION, keyword)
  }
}

@Serializable internal data class DatabaseSearchQueryPlan(val query: String)

internal fun resolveDatabaseSearchQuery(
  request: DatabaseSearchQueryRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanDatabaseSearchQueryV1,
): String =
  invokeLibraryRule<DatabaseSearchQueryPlan>(
      rustCbor.encodeToByteArray(request),
      "database search query",
      nativeCall,
    )
    .query

@Serializable
internal data class QuickSearchSnapshot(
  val mode: Int,
  val category: Int,
  val keyword: String?,
  @SerialName("advancedSearch") val advancedSearch: Int,
  @SerialName("minimumRating") val minimumRating: Int,
  @SerialName("pageFrom") val pageFrom: Int,
  @SerialName("pageTo") val pageTo: Int,
) {
  companion object {
    fun from(search: QuickSearch) =
      QuickSearchSnapshot(
        search.mode,
        search.category,
        search.keyword,
        search.advanceSearch,
        search.minRating,
        search.pageFrom,
        search.pageTo,
      )
  }
}

@Serializable
internal data class QuickSearchMatchRequest(
  val version: Int,
  val current: QuickSearchSnapshot,
  val saved: QuickSearchSnapshot?,
)

@Serializable internal data class QuickSearchMatchPlan(val matches: Boolean)

internal fun resolveQuickSearchMatch(
  request: QuickSearchMatchRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanQuickSearchMatchV1,
): Boolean =
  invokeLibraryRule<QuickSearchMatchPlan>(
      rustCbor.encodeToByteArray(request),
      "QuickSearch match",
      nativeCall,
    )
    .matches

@Serializable
internal data class LocalFavoriteMutationRequest(
  val version: Int,
  @SerialName("requestedSlot") val requestedSlot: Int,
  @SerialName("currentSlot") val currentSlot: Int,
  @SerialName("localFavorited") val localFavorited: Boolean,
)

@Serializable
internal enum class LocalFavoriteAction {
  @SerialName("removeCloud") REMOVE_CLOUD,
  @SerialName("removeLocal") REMOVE_LOCAL,
  @SerialName("addLocal") ADD_LOCAL,
  @SerialName("setCloud") SET_CLOUD,
}

@Serializable
internal data class LocalFavoriteMutationPlan(
  val action: LocalFavoriteAction,
  @SerialName("nextFavoriteSlot") val nextFavoriteSlot: Int,
  @SerialName("cloudSlot") val cloudSlot: Int?,
  val added: Boolean,
)

internal fun resolveLocalFavoriteMutation(
  request: LocalFavoriteMutationRequest,
  nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int = ::nativePlanLocalFavoriteMutationV1,
): LocalFavoriteMutationPlan {
  val plan =
    invokeLibraryRule<LocalFavoriteMutationPlan>(
      rustCbor.encodeToByteArray(request),
      "local favorite mutation",
      nativeCall,
    )
  require(plan.nextFavoriteSlot in NOT_FAVORITED_SLOT..9) {
    "Invalid next favorite slot"
  }
  require((plan.action == LocalFavoriteAction.SET_CLOUD) == (plan.cloudSlot != null)) {
    "Invalid local favorite action shape"
  }
  plan.cloudSlot?.let { require(it in 0..9) { "Invalid cloud favorite slot" } }
  return plan
}

private inline fun <reified T> invokeLibraryRule(
  request: ByteArray,
  operation: String,
  noinline nativeCall: (ByteBuffer, Int, ByteBuffer) -> Int,
): T {
  val envelope =
    invokeRustCbor<ApiEnvelope<T>>(
      request,
      operation = "Library Rule $operation",
      nativeCall = nativeCall,
    )
  require(envelope.version == LIBRARY_RULE_API_VERSION) {
    "Unsupported Library Rule API version"
  }
  require(envelope.module == LIBRARY_RULE_MODULE) { "Unexpected Library Rule module" }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Missing Library Rule payload" }
}

private external fun nativePlanDatabaseSearchQueryV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private external fun nativePlanQuickSearchMatchV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

private external fun nativePlanLocalFavoriteMutationV1(
  request: ByteBuffer,
  requestSize: Int,
  response: ByteBuffer,
): Int

internal const val LIBRARY_RULE_API_VERSION = 1
internal const val LIBRARY_RULE_MODULE = "libraryRule"
private const val NOT_FAVORITED_SLOT = -2
