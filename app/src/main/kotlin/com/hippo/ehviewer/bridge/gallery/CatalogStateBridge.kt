@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import com.hippo.ehviewer.bridge.invokeRustCborBytes
import com.hippo.ehviewer.bridge.rustCbor
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

@Serializable
internal data class CatalogNavigation(
  @ByteString val previous: ByteArray?,
  @ByteString val next: ByteArray?,
  @ByteString val seek: ByteArray?,
  val range: Int,
) {
  companion object {
    fun from(previous: String?, next: String?, seek: String?, range: Int = 0) =
      CatalogNavigation(
        previous?.toUtf16LeBytes(),
        next?.toUtf16LeBytes(),
        seek?.toUtf16LeBytes(),
        range,
      )
  }
}

internal fun ByteArray.catalogText(): String {
  require(size <= 16 * 1024 && size % 2 == 0) { "Invalid catalog text" }
  return CharArray(size / 2) {
      ((this[it * 2].toInt() and 255) or ((this[it * 2 + 1].toInt() and 255) shl 8)).toChar()
    }
    .concatToString()
}

@Serializable
internal enum class CatalogTransition {
  @SerialName("index") INDEX,
  @SerialName("range") RANGE,
  @SerialName("seek") SEEK,
  @SerialName("page") PAGE,
  @SerialName("savedProgress") SAVED_PROGRESS,
}

@Serializable
internal data class CatalogNavigationRequest(
  val version: Int,
  val state: CatalogNavigation,
  val action: CatalogTransition,
  @ByteString val text: ByteArray?,
  val value: Int,
  val forward: Boolean,
)

internal fun transitionCatalog(
  state: CatalogNavigation,
  action: CatalogTransition,
  text: String? = null,
  value: Int = 0,
  forward: Boolean = false,
  nativeCall: (ByteBuffer, Int) -> ByteArray = ::nativeCatalogNavigationV1,
): CatalogNavigation {
  val request =
    CatalogNavigationRequest(
      version = 1,
      state = state,
      action = action,
      text = text?.toUtf16LeBytes(),
      value = value,
      forward = forward,
    )
  require(
    listOf(state.previous, state.next, state.seek, request.text).all {
      it == null || (it.size <= 16 * 1024 && it.size % 2 == 0)
    }
  ) {
    "Invalid catalog navigation text"
  }
  val envelope =
    invokeRustCborBytes<ApiEnvelope<CatalogNavigation>>(
      rustCbor.encodeToByteArray(request),
      "Catalog navigation",
      maxResponseSize = 128 * 1024,
      nativeCall = nativeCall,
    )
  require(envelope.version == 1 && envelope.module == "catalogState") {
    "Invalid catalog navigation envelope"
  }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload).also { plan ->
    listOf(plan.previous, plan.next, plan.seek).forEach { it?.catalogText() }
  }
}

internal fun savedSearchProgress(name: String) =
  transitionCatalog(
      CatalogNavigation.from(null, null, null),
      CatalogTransition.SAVED_PROGRESS,
      name,
    )
    .next
    ?.catalogText()

/** JVM parses editable numbers; Rust owns subsequent arithmetic and business bounds. */
internal object CatalogNumbers {
  fun quickSearchAllowed(mode: Int) = scalar(10, mode) == 1

  fun toggle(value: Int, bit: Int) = scalar(0, value, bit)

  fun ratingIndex(value: Int) = scalar(1, value)

  fun ratingValue(index: Int) = scalar(2, index)

  fun minimumPages(value: Int) = scalar(3, value)

  fun maximumPages(value: Int) = scalar(4, value)

  fun pageRangeError(minimum: Int, maximum: Int) = scalar(5, minimum, maximum)

  fun validToplistPage(value: Int) = scalar(6, value) == 1

  fun displayPage(zeroBased: Int) = scalar(8, zeroBased)

  val toplistPages
    get() = scalar(9, 0)

  fun neighbors(page: Int): Pair<Int?, Int?> {
    val result = nativeCatalogNumbersV1(7, page, 0)
    require(result.size == 4 && result[0] in 0..1 && result[2] in 0..1) {
      "Invalid toplist neighbors"
    }
    return result[1].takeIf { result[0] == 1 } to result[3].takeIf { result[2] == 1 }
  }

  private fun scalar(op: Int, left: Int, right: Int = 0): Int {
    val result = nativeCatalogNumbersV1(op, left, right)
    require(result.size == 1) { "Invalid catalog number" }
    return result[0]
  }
}

private external fun nativeCatalogNavigationV1(request: ByteBuffer, size: Int): ByteArray

private external fun nativeCatalogNumbersV1(operation: Int, left: Int, right: Int): IntArray
