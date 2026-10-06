@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.hippo.ehviewer.bridge.archive

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.spider.ComicInfo
import java.nio.ByteBuffer
import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlinx.serialization.Serializable
import kotlinx.serialization.cbor.ByteString
import kotlinx.serialization.encodeToByteArray

class ComicInfoBridgeTest {
  @Test
  fun decodesRustComicInfoWireNames() {
    val decoded =
      buildComicInfo(galleryRequest()) { _, _, responseBuffer ->
        responseBuffer.put(RUST_BUILD_RESPONSE)
        RUST_BUILD_RESPONSE.size
      }

    assertEquals(comicInfo(), decoded)
  }

  @Test
  fun everyNativeRequestEmitsItsRequiredProtocolVersion() {
    val requests = mutableListOf<String>()
    val info = comicInfo()
    val built =
      buildComicInfo(galleryRequest()) { requestBuffer, requestSize, responseBuffer ->
        requests += requestBuffer.readHex(requestSize)
        respond(ApiEnvelope(1, "comicInfo", info), responseBuffer)
      }

    assertEquals(info, built)

    val xml = "<ComicInfo />".encodeToByteArray()
    val encoded =
      encodeComicInfoXml(info) { requestBuffer, requestSize, responseBuffer ->
        requests += requestBuffer.readHex(requestSize)
        respond(ApiEnvelope(1, "comicInfo", ComicInfoXmlWire(xml)), responseBuffer)
      }

    assertContentEquals(xml, encoded)

    val decoded =
      decodeComicInfoXml(xml) { requestBuffer, requestSize, responseBuffer ->
        requests += requestBuffer.readHex(requestSize)
        respond(ApiEnvelope(1, "comicInfo", info), responseBuffer)
      }

    assertEquals(info, decoded)
    assertEquals(EXPECTED_REQUESTS, requests)
  }

  private fun galleryRequest() =
    GalleryComicInfoRequest(
      version = COMIC_INFO_API_VERSION,
      title = "Series",
      titleJpn = "Alternate",
      web = "https://example.invalid/g/1/token/",
      rating = 4.5f,
      pages = 2,
      simpleLanguage = "en",
      simpleTags = listOf("artist:name"),
      tagGroups =
        listOf(
          ComicInfoTagGroup(
            namespace = "artist",
            tags = listOf(ComicInfoTag("name", ComicInfoTagPower.SOLID)),
          )
        ),
    )

  private fun comicInfo() =
    ComicInfo(
      series = "Series",
      alternateSeries = "Alternate",
      writer = null,
      penciller = listOf("name"),
      genre = null,
      web = "https://example.invalid/g/1/token/",
      pageCount = 2,
      languageISO = "en",
      characters = null,
      teams = null,
      communityRating = "4.5",
      simpleTags = listOf("name"),
    )

  private inline fun <reified T> respond(value: T, responseBuffer: ByteBuffer): Int {
    val bytes = rustCbor.encodeToByteArray(value)
    responseBuffer.put(bytes)
    return bytes.size
  }

  private fun ByteBuffer.readHex(size: Int) =
    ByteArray(size).also(::get).joinToString("") {
      it.toUByte().toString(16).padStart(2, '0')
    }

  @Serializable private data class ComicInfoXmlWire(@ByteString val xml: ByteArray)

  private companion object {
    val EXPECTED_REQUESTS =
      listOf(
        "a96776657273696f6e01657469746c6566536572696573687469746c654a706e69416c7465726e61746563776562782268747470733a2f2f6578616d706c652e696e76616c69642f672f312f746f6b656e2f66726174696e67fa40900000657061676573026e73696d706c654c616e677561676562656e6a73696d706c6554616773816b6172746973743a6e616d656974616747726f75707381a2696e616d65737061636566617274697374647461677381a26474657874646e616d6565706f77657265736f6c6964",
        "a26776657273696f6e0164696e666fac66736572696573665365726965736f616c7465726e61746553657269657369416c7465726e61746566777269746572f66970656e63696c6c657281646e616d656567656e7265f663776562782268747470733a2f2f6578616d706c652e696e76616c69642f672f312f746f6b656e2f6970616765436f756e74026b6c616e677561676549736f62656e6a63686172616374657273f6657465616d73f66f636f6d6d756e697479526174696e6763342e356a73696d706c655461677381646e616d65",
        "a26776657273696f6e0163786d6c4d3c436f6d6963496e666f202f3e",
      )

    val RUST_BUILD_RESPONSE =
      "a46776657273696f6e01666d6f64756c6569636f6d6963496e666f677061796c6f6164ac66736572696573665365726965736f616c7465726e61746553657269657369416c7465726e61746566777269746572f66970656e63696c6c657281646e616d656567656e7265f663776562782268747470733a2f2f6578616d706c652e696e76616c69642f672f312f746f6b656e2f6970616765436f756e74026b6c616e677561676549736f62656e6a63686172616374657273f6657465616d73f66f636f6d6d756e697479526174696e6763342e356a73696d706c655461677381646e616d65656572726f72f6"
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
  }
}
