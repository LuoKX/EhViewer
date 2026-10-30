package com.hippo.ehviewer.bridge.reader

internal fun readerNavigation(
  operation: Int,
  facts: IntArray,
  nativeCall: (Int, IntArray) -> IntArray = ::nativeReaderNavigationV1,
): IntArray {
  require(facts.size <= 1024 * 1024) { "Reader navigation request exceeds limit" }
  return nativeCall(operation, facts).also { result ->
    val valid =
      when (operation) {
        0 -> result.size == 2 && result[0] in 0..1 && result[1] in 0..2
        1 -> result.size == 4 && result[0] in 1..5 && result.drop(1).all { it in 0..1 }
        2 ->
          result.size <= 20 &&
            result.size % 5 == 0 &&
            result.toList().chunked(5).all {
              it[0] in 1..4 && it.drop(1).all { bits -> Float.fromBits(bits).isFinite() }
            }
        3,
        9 -> result.size == 2 && result.all { it in 0..(if (operation == 3) 2 else 1) }
        4,
        7 -> result.size == 1 && result[0] in 0..1
        5,
        8 -> result.size == 1 && result[0] in 0..2
        6 -> result.size == 2 && result[0] in 0..1
        10 -> result.size == 1 && result[0] in 1..5
        else -> false
      }
    require(valid) { "Invalid reader navigation plan" }
  }
}

internal fun Boolean.readerFact() = if (this) 1 else 0

internal fun readerMode(preference: Int, tags: List<String>?): IntArray {
  val size = 1L + tags.orEmpty().sumOf { it.length.toLong() + 1 }
  require(size <= 1024 * 1024)
  val facts = IntArray(size.toInt())
  facts[0] = preference
  var offset = 1
  for (tag in tags.orEmpty()) {
    facts[offset++] = tag.length
    for (unit in tag) facts[offset++] = unit.code
  }
  return readerNavigation(1, facts)
}

internal data class ReaderGallerySource<T>(val page: Int, val archive: T?)

/** Preserve progress-before-archive lookup and explicit-page short-circuiting. */
internal suspend fun <T> readerGallerySource(
  requestedPage: Int,
  readProgress: suspend () -> Int,
  findArchive: () -> T?,
): ReaderGallerySource<T> {
  val initial = readerNavigation(0, intArrayOf(0, requestedPage, 0))
  val page = if (initial[0] == 1) readProgress() else requestedPage
  val archive = findArchive()
  val source = readerNavigation(0, intArrayOf(0, page, (archive != null).readerFact()))[1]
  require(source != 1 || archive != null)
  return ReaderGallerySource(page, if (source == 1) archive else null)
}

internal fun readerArchiveSorting(hasInfo: Boolean, filename: String): Boolean =
  readerNavigation(7, intArrayOf(hasInfo.readerFact()) + filename.map(Char::code))[0] == 1

internal fun readerPageTarget(current: Int, count: Int, previous: Boolean): Int? {
  val result = readerNavigation(6, intArrayOf(current, count, if (previous) 1 else 2))
  return result[1].takeIf { result[0] == 1 }
}

/** Pan observations remain lazy: menu/webtoon taps never inspect Telephoto bounds. */
internal suspend fun executeReaderTap(
  intent: IntArray,
  panAvailable: () -> Boolean,
  remaining: (Int) -> Float,
  pan: suspend (Int) -> Unit,
  move: suspend (Int) -> Unit,
  menu: () -> Unit,
) {
  require(intent.size == 2 && intent.all { it in 0..2 })
  val (direction, side) = intent
  if (side != 0) {
    val available = panAvailable()
    val distance = if (available) remaining(side) else 0f
    if (readerNavigation(4, intArrayOf(available.readerFact(), distance.toRawBits()))[0] == 1) {
      pan(side)
      return
    }
  }
  if (direction == 0) menu() else move(direction)
}

private external fun nativeReaderNavigationV1(operation: Int, facts: IntArray): IntArray
