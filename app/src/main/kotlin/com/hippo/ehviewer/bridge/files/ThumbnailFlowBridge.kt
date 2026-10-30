package com.hippo.ehviewer.bridge.files

internal fun thumbnailPlan(
  phase: Int,
  facts: IntArray,
  nativeCall: (Int, IntArray) -> IntArray = ::nativeThumbnailFlowV1,
): IntArray {
  require(facts.size <= 1024 * 1024)
  return nativeCall(phase, facts).also { result ->
    require(
      when (phase) {
        0,
        1 -> result.size == 1 && result[0] in 0..1
        2 -> result.size == 1 && (result[0] == -1 || result[0] in 0 until facts[0])
        3 -> result.size == 2 && result[0] in 0..1 && result[1] == 0
        4 ->
          result.size == 2 &&
            result[0] in listOf(0, 2) &&
            result[1] in 0..1 &&
            (result[0] == 2 || result[1] == 0)
        5 -> result.size == 2 && result[0] in 2..3 && result[1] == 0
        6 -> result.size == 2 && result[0] == 2 && result[1] in 0..1
        else -> false
      }
    ) {
      "Invalid thumbnail plan"
    }
  }
}

private fun Boolean.fact() = if (this) 1 else 0

internal fun usesDownloadedThumbnail(hasInfo: Boolean, dirnameBlank: Boolean) =
  thumbnailPlan(0, intArrayOf(hasInfo.fact(), dirnameBlank.fact()))[0] == 1

/**
 * Metadata write precedes directory construction and suffix validation, even on an invalid suffix.
 */
internal suspend fun <D> prepareDownloadedThumbnail(
  oldKey: String?,
  key: String,
  update: suspend () -> Unit,
  directory: () -> D,
): Pair<D, String> {
  if (thumbnailPlan(1, intArrayOf((oldKey == key).fact()))[0] == 1) update()
  val dir = directory()
  val whitespace = key.filter(Char::isWhitespace).map(Char::code).distinct()
  require(key.length.toLong() + whitespace.size + 1 <= 1024 * 1024)
  val start = thumbnailPlan(2, intArrayOf(key.length) + key.map(Char::code) + whitespace)[0]
  check(start >= 0)
  return dir to "thumb.${key.substring(start)}"
}

/** Advance only after a completed effect. A cache miss completes writeCache and still cleans up. */
internal suspend fun <R> executeDownloadedThumbnail(
  localExists: () -> Boolean,
  local: suspend () -> R,
  remote: suspend () -> R,
  success: (R) -> Boolean,
  directoryExists: () -> Boolean,
  differentOldPath: () -> Boolean,
  writeCache: suspend () -> Unit,
  deleteOld: () -> Unit,
): R {
  if (thumbnailPlan(3, intArrayOf(localExists().fact()))[0] == 1) {
    val result = local()
    val succeeded = success(result)
    val plan =
      thumbnailPlan(4, intArrayOf(succeeded.fact(), (succeeded && differentOldPath()).fact()))
    if (plan[1] == 1) deleteOld()
    if (plan[0] == 2) return result
  }
  val result = remote()
  val succeeded = success(result)
  if (
    thumbnailPlan(5, intArrayOf(succeeded.fact(), (succeeded && directoryExists()).fact()))[0] == 3
  ) {
    writeCache()
    if (thumbnailPlan(6, intArrayOf(differentOldPath().fact()))[1] == 1) deleteOld()
  }
  return result
}

private external fun nativeThumbnailFlowV1(phase: Int, facts: IntArray): IntArray
