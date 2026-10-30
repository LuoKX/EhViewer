package com.hippo.ehviewer.bridge.gallery

private val platformDecimalDigit = Regex("\\d")

/** Facts from the running platform's regex and integer parser, not a second URL grammar. */
internal fun commentDigitFacts(text: String): ByteArray {
  require(text.length <= 128 * 1024) { "Comment text exceeds limit" }
  val result =
    ByteArray(text.length) {
      Character.digit(text[it], 10).let { value -> if (value < 0) 15 else value }.toByte()
    }
  platformDecimalDigit.findAll(text).forEach { match ->
    for (index in match.range) result[index] = (result[index].toInt() or 16).toByte()
  }
  return result
}

internal data class CommentLink(val start: Int, val end: Int)

internal fun commentLinks(
  text: String,
  nativeCall: (CharArray, ByteArray) -> IntArray = ::nativeCommentLinksV1,
): List<CommentLink> {
  val facts = commentDigitFacts(text)
  val result = nativeCall(text.toCharArray(), facts)
  require(result.size % 2 == 0 && result.size <= text.length) { "Invalid Rust comment ranges" }
  var previousEnd = 0
  return result.asList().chunked(2).map { (start, end) ->
    require(start >= previousEnd && start < end && end <= text.length) {
      "Invalid Rust comment range"
    }
    previousEnd = end
    CommentLink(start, end)
  }
}

internal fun commentImageDimensions(
  source: String,
  nativeCall: (CharArray, ByteArray) -> IntArray? = ::nativeCommentImageDimensionsV1,
): IntArray? {
  val facts = commentDigitFacts(source)
  return nativeCall(source.toCharArray(), facts).also {
    require(it == null || it.size == 4) { "Invalid Rust comment dimensions" }
  }
}

private external fun nativeCommentLinksV1(text: CharArray, digits: ByteArray): IntArray

private external fun nativeCommentImageDimensionsV1(text: CharArray, digits: ByteArray): IntArray?
