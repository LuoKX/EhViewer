@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.invokeRustCbor
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.spider.ComicInfo
import java.nio.ByteBuffer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

internal const val COMIC_INFO_API_VERSION = 1
private const val COMIC_INFO_MODULE = "comicInfo"

@Serializable
internal enum class ComicInfoTagPower {
  @SerialName("solid") SOLID,
  @SerialName("active") ACTIVE,
  @SerialName("weak") WEAK,
}

@Serializable internal data class ComicInfoTag(val text: String, val power: ComicInfoTagPower)

@Serializable
internal data class ComicInfoTagGroup(
  val namespace: String,
  val tags: List<ComicInfoTag>,
)

@Serializable
internal data class GalleryComicInfoRequest(
  val version: Int = COMIC_INFO_API_VERSION,
  val title: String?,
  @SerialName("titleJpn") val titleJpn: String? = null,
  val web: String,
  val rating: Float,
  val pages: Int,
  @SerialName("simpleLanguage") val simpleLanguage: String? = null,
  @SerialName("simpleTags") val simpleTags: List<String>? = null,
  @SerialName("tagGroups") val tagGroups: List<ComicInfoTagGroup>? = null,
)

@Serializable
private data class ComicInfoEncodeRequest(
  val version: Int = COMIC_INFO_API_VERSION,
  val info: ComicInfo,
)

@Serializable
private data class ComicInfoDecodeRequest(
  val version: Int = COMIC_INFO_API_VERSION,
  @ByteString val xml: ByteArray,
)

@Serializable private data class ComicInfoXml(@ByteString val xml: ByteArray)

internal typealias ComicInfoNativeCall = (ByteBuffer, Int, ByteBuffer) -> Int

internal fun buildComicInfo(
  request: GalleryComicInfoRequest,
  nativeCall: ComicInfoNativeCall = ::nativeBuildComicInfoV1,
): ComicInfo =
  decodeComicInfoEnvelope(
    request = rustCbor.encodeToByteArray(request),
    operation = "ComicInfo Build",
    nativeCall = nativeCall,
  )

internal fun encodeComicInfoXml(
  info: ComicInfo,
  nativeCall: ComicInfoNativeCall = ::nativeEncodeComicInfoXmlV1,
): ByteArray =
  decodeComicInfoEnvelope<ComicInfoXml>(
      request = rustCbor.encodeToByteArray(ComicInfoEncodeRequest(info = info)),
      operation = "ComicInfo Encode",
      nativeCall = nativeCall,
    )
    .xml

internal fun decodeComicInfoXml(
  xml: ByteArray,
  nativeCall: ComicInfoNativeCall = ::nativeDecodeComicInfoXmlV1,
): ComicInfo =
  decodeComicInfoEnvelope(
    request = rustCbor.encodeToByteArray(ComicInfoDecodeRequest(xml = xml)),
    operation = "ComicInfo Decode",
    nativeCall = nativeCall,
  )

private inline fun <reified T> decodeComicInfoEnvelope(
  request: ByteArray,
  operation: String,
  noinline nativeCall: ComicInfoNativeCall,
): T {
  val envelope = invokeRustCbor<ApiEnvelope<T>>(request, operation, nativeCall)
  require(envelope.version == COMIC_INFO_API_VERSION) { "Unsupported ComicInfo API version" }
  require(envelope.module == COMIC_INFO_MODULE) { "Unexpected Rust module: ${envelope.module}" }
  envelope.error?.let { throw RustFailureException(it) }
  return checkNotNull(envelope.payload) { "Missing ComicInfo payload" }
}

private external fun nativeBuildComicInfoV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

private external fun nativeEncodeComicInfoXmlV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int

private external fun nativeDecodeComicInfoXmlV1(
  requestBuffer: ByteBuffer,
  requestLength: Int,
  responseBuffer: ByteBuffer,
): Int
