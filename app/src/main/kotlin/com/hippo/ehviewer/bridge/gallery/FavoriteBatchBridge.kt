package com.hippo.ehviewer.bridge.gallery

import com.ehviewer.core.model.GalleryInfo

internal enum class FavoriteBatchAction {
  NONE,
  DELETE_LOCAL,
  DELETE_CLOUD,
  UPLOAD_THEN_REMOVE_LOCAL,
  SAVE_LOCAL,
  MOVE_CLOUD,
}

internal data class FavoriteBatchPlan(val action: FavoriteBatchAction, val refresh: Boolean)

internal fun planFavoriteBatch(
  source: Int,
  destination: Int? = null,
  nativeCall: (Int, Int, Int) -> IntArray = ::nativeFavoriteBatchV1,
): FavoriteBatchPlan {
  val result = nativeCall(if (destination == null) 0 else 1, source, destination ?: 0)
  require(
    result.size == 2 && result[0] in FavoriteBatchAction.entries.indices && result[1] in 0..1
  ) {
    "Invalid favorite batch plan"
  }
  val action = FavoriteBatchAction.entries[result[0]]
  require((action == FavoriteBatchAction.NONE) == (result[1] == 0)) {
    "Invalid favorite refresh plan"
  }
  require(
    if (destination == null)
      action == FavoriteBatchAction.DELETE_LOCAL || action == FavoriteBatchAction.DELETE_CLOUD
    else action != FavoriteBatchAction.DELETE_LOCAL && action != FavoriteBatchAction.DELETE_CLOUD
  )
  require(
    when (action) {
      FavoriteBatchAction.NONE -> source == destination
      FavoriteBatchAction.DELETE_LOCAL -> source == -2
      FavoriteBatchAction.DELETE_CLOUD -> source != -2
      FavoriteBatchAction.UPLOAD_THEN_REMOVE_LOCAL ->
        source == -2 && destination != null && destination in 0..9
      FavoriteBatchAction.SAVE_LOCAL -> source != -2 && destination == -2
      FavoriteBatchAction.MOVE_CLOUD ->
        source != -2 && destination != null && destination in 0..9 && source != destination
    }
  ) {
    "Favorite intent does not match selected stores"
  }
  return FavoriteBatchPlan(action, result[1] == 1)
}

internal fun selectedBulkFavoriteDestination(index: Int, menuSize: Int): Int {
  val result = nativeFavoriteBatchV1(2, index, menuSize)
  require(result.size == 1 && (result[0] == -2 || result[0] in 0..9)) {
    "Invalid bulk favorite destination"
  }
  return result[0]
}

/** A throwing/cancelled upload exits before removal; no compensation or parallel requests. */
internal suspend fun executeFavoriteBatch(
  plan: FavoriteBatchPlan,
  removeLocal: suspend () -> Unit,
  removeCloud: suspend () -> Unit,
  upload: suspend () -> Unit,
  saveLocal: suspend () -> Unit,
  moveCloud: suspend () -> Unit,
) {
  when (plan.action) {
    FavoriteBatchAction.NONE -> Unit
    FavoriteBatchAction.DELETE_LOCAL -> removeLocal()
    FavoriteBatchAction.DELETE_CLOUD -> removeCloud()
    FavoriteBatchAction.UPLOAD_THEN_REMOVE_LOCAL -> {
      upload()
      removeLocal()
    }
    FavoriteBatchAction.SAVE_LOCAL -> saveLocal()
    FavoriteBatchAction.MOVE_CLOUD -> moveCloud()
  }
}

/** Observe each gallery immediately before its existing short-circuit lookup. */
internal suspend fun applyLocalFavoriteOverlay(
  info: GalleryInfo,
  containsLocal: suspend (Long) -> Boolean,
) {
  val result = nativeFavoriteBatchV1(3, info.favoriteSlot, 0)
  require(result.size == 2 && result[0] in 0..1 && result[1] == GalleryInfo.LOCAL_FAVORITED) {
    "Invalid favorite overlay plan"
  }
  if (result[0] == 1 && containsLocal(info.gid)) info.favoriteSlot = result[1]
}

private external fun nativeFavoriteBatchV1(operation: Int, left: Int, right: Int): IntArray
