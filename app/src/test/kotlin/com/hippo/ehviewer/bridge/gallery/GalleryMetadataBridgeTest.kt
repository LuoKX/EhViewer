package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.GalleryMetadataBatch
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryMetadataBridgeTest {
  @Test
  fun decodesTheRustGeneratedGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<GalleryMetadataBatch>>(V1_GOLDEN_CBOR_HEX.decodeHex())

    val batch = envelope.toGalleryMetadataBatch()
    assertEquals(2, batch.items.size)
    with(batch.items[0]) {
      assertEquals(101, galleryId)
      assertEquals("Rock & Roll &unknown; &lt;", title)
      assertEquals("日本 <特集> \"一\"", titleJapanese)
      assertEquals(0x8, category)
      assertEquals("ab/c.jpg", thumbnailKey)
      assertEquals("Alice 'A'\u00a0× &mystery;", uploader)
      assertEquals(1_700_000_000, posted)
      assertEquals(4.75F, rating)
      assertEquals(listOf("language:english", "artist:sample"), tags)
      assertEquals(12, pageCount)
    }
    assertEquals((-0.0F).toBits(), batch.items[1].rating.toBits())
    assertEquals(Int.MIN_VALUE, batch.items[1].pageCount)
  }

  @Test
  fun rejectsVersionAndModuleDrift() {
    val envelope = successEnvelope()

    assertFailsWith<IllegalArgumentException> {
      envelope.copy(version = 2).toGalleryMetadataBatch()
    }
    assertFailsWith<IllegalArgumentException> {
      envelope.copy(module = "galleryRate").toGalleryMetadataBatch()
    }
  }

  @Test
  fun preservesTypedRustFailuresForTheSharedHttpErrorMapper() {
    val failure = RustError(RustErrorKind.ERROR, "Gallery metadata response is invalid")
    val exception =
      assertFailsWith<RustFailureException> {
        ApiEnvelope<GalleryMetadataBatch>(
            version = 1,
            module = "galleryMetadata",
            error = failure,
          )
          .toGalleryMetadataBatch()
      }

    assertEquals(failure, exception.failure)
  }

  private fun successEnvelope() =
    ApiEnvelope(version = 1, module = "galleryMetadata", payload = GalleryMetadataBatch())

  private companion object {
    const val V1_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656f67616c6c6572794d657461646174" +
        "61677061796c6f6164a1656974656d7382aa636769641865657469746c65781a52" +
        "6f636b202620526f6c6c2026756e6b6e6f776e3b20266c743b687469746c654a" +
        "706e75e697a5e69cac203ce789b9e99b863e2022e4b880226863617465676f72" +
        "7908687468756d624b65796861622f632e6a70676875706c6f6164657277416c" +
        "69636520274127c2a0c39720266d7973746572793b66706f737465641a6553f100" +
        "66726174696e67f944c06a73696d706c655461677382706c616e67756167653a" +
        "656e676c6973686d6172746973743a73616d706c656570616765730caa63676964" +
        "26657469746c6560687469746c654a706e606863617465676f7279190800687468" +
        "756d624b6579676b65792e6a70676875706c6f61646572f666706f7374656420" +
        "66726174696e67f980006a73696d706c6554616773806570616765733a7fffffff" +
        "656572726f72f6"
  }
}
