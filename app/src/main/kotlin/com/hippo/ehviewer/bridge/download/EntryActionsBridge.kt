package com.hippo.ehviewer.bridge.download

internal fun partitionDownloadEntries(existing: BooleanArray): Pair<List<Int>, List<Int>> {
  require(existing.size <= 100_000) { "Entry observations exceed limit" }
  val result = nativeEntryActionV1(0, 0, 0, existing)
  require(result.size == existing.size + 1 && result[0] in 0..existing.size) {
    "Invalid entry partition size"
  }
  val slots = result.drop(1)
  require(slots.toSet() == existing.indices.toSet()) { "Invalid entry partition slots" }
  val old = slots.take(result[0])
  val new = slots.drop(result[0])
  require(
    old.all { existing[it] } &&
      new.all { !existing[it] } &&
      listOf(old, new).all { it.zipWithNext().all { (a, b) -> a < b } }
  ) {
    "Invalid entry partition order"
  }
  return old to new
}

internal data class DownloadEntryChoice(val direct: Boolean, val useDefault: Boolean)

internal fun downloadEntryChoice(
  force: Boolean,
  hasDefault: Boolean,
  defaultNull: Boolean,
  defaultExists: Boolean,
  emptyLabels: Boolean,
): DownloadEntryChoice {
  val result =
    nativeEntryActionV1(
      1,
      0,
      0,
      booleanArrayOf(force, hasDefault, defaultNull, defaultExists, emptyLabels),
    )
  require(result.size == 2 && result.all { it in 0..1 }) { "Invalid download entry plan" }
  return DownloadEntryChoice(result[0] == 1, result[1] == 1)
}

internal data class FavoriteEntryChoice(
  val chooser: Boolean,
  val removeRow: Boolean,
  val fetchNote: Boolean,
  val slot: Int,
)

internal fun favoriteEntryChoice(
  signedIn: Boolean,
  currentSlot: Int,
  defaultSlot: Int,
  noteMissing: Boolean,
): FavoriteEntryChoice {
  val result =
    nativeEntryActionV1(2, currentSlot, defaultSlot, booleanArrayOf(signedIn, noteMissing))
  require(result.size == 4 && result.take(3).all { it in 0..1 }) { "Invalid favorite entry plan" }
  return FavoriteEntryChoice(result[0] == 1, result[1] == 1, result[2] == 1, result[3])
}

internal fun selectedFavoriteSlot(removeRow: Boolean, index: Int): Int {
  val result = nativeEntryActionV1(3, index, 0, booleanArrayOf(removeRow))
  require(result.size == 1 && result[0] in -2..9) { "Invalid favorite selection" }
  return result[0]
}

/** Menu includes its localized default row; all other strings are captured before the dialog. */
internal fun selectedDownloadLabel(menu: List<String>, index: Int): String? {
  val result = nativeEntryActionV1(4, index, menu.size - 1, booleanArrayOf())
  require(result.size == 1 && result[0] in -1 until menu.size - 1) { "Invalid label selection" }
  return result[0].takeIf { it >= 0 }?.let { menu[it + 1] }
}

private external fun nativeEntryActionV1(
  operation: Int,
  left: Int,
  right: Int,
  facts: BooleanArray,
): IntArray
