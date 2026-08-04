package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.bridge.rustCbor
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.decodeFromByteArray
import kotlinx.serialization.encodeToByteArray

class GalleryArchiveRequestBridgeTest {
  @Test
  fun requestAndResponseGoldensMatchRustForListAndDownload() {
    fixtures.forEach { fixture ->
      assertEquals(fixture.requestGolden, rustCbor.encodeToByteArray(fixture.request).toHex())
      val envelope =
        rustCbor.decodeFromByteArray<ApiEnvelope<GalleryArchiveRequestPlan>>(
          fixture.responseGolden.decodeHex()
        )
      assertEquals(fixture.expected, envelope.toGalleryArchiveRequestPlan(fixture.expected.kind))
    }
  }

  @Test
  fun nativeCallbackReceivesEachCompleteRequestExactlyOnce() {
    fixtures.forEach { fixture ->
      var calls = 0
      val actual =
        planGalleryArchiveRequest(fixture.request) { requestBuffer, requestSize, responseBuffer ->
          calls += 1
          val requestBytes = ByteArray(requestSize).also { requestBuffer.get(it) }
          assertEquals(fixture.requestGolden, requestBytes.toHex())
          assertEquals(
            fixture.request,
            rustCbor.decodeFromByteArray<GalleryArchiveRequest>(requestBytes),
          )
          val responseBytes = fixture.responseGolden.decodeHex()
          responseBuffer.put(responseBytes)
          responseBytes.size
        }

      assertEquals(fixture.expected, actual)
      assertEquals(1, calls)
    }
  }

  @Test
  fun selectorExecutesOnlyTheSelectedPlanner() {
    var legacyCalls = 0
    var settingCalls = 0
    var rustCalls = 0
    val legacySelected =
      selectGalleryArchiveRequestPlan(
        legacyPlanner = {
          legacyCalls += 1
          listPlan
        },
        rustEnabled = {
          settingCalls += 1
          false
        },
        rustPlanner = {
          rustCalls += 1
          listPlan
        },
      )

    assertEquals(listPlan, legacySelected)
    assertEquals(1, legacyCalls)
    assertEquals(1, settingCalls)
    assertEquals(0, rustCalls)

    assertEquals(
      listPlan,
      selectGalleryArchiveRequestPlan(
        legacyPlanner = {
          legacyCalls += 1
          listPlan
        },
        rustEnabled = {
          settingCalls += 1
          true
        },
        rustPlanner = {
          rustCalls += 1
          listPlan
        },
      ),
    )
    assertEquals(1, legacyCalls)
    assertEquals(2, settingCalls)
    assertEquals(1, rustCalls)
  }

  @Test
  fun selectorPropagatesTheSelectedPlannerFailureWithoutFallback() {
    var legacyCalls = 0
    assertFailsWith<IllegalStateException> {
      selectGalleryArchiveRequestPlan(
        legacyPlanner = {
          legacyCalls += 1
          listPlan
        },
        rustEnabled = { true },
        rustPlanner = { throw IllegalStateException("rust") },
      )
    }
    assertFailsWith<LinkageError> {
      selectGalleryArchiveRequestPlan(
        legacyPlanner = {
          legacyCalls += 1
          listPlan
        },
        rustEnabled = { true },
        rustPlanner = { throw LinkageError("rust") },
      )
    }
    assertEquals(0, legacyCalls)

    assertFailsWith<IllegalStateException> {
      selectGalleryArchiveRequestPlan(
        legacyPlanner = { throw IllegalStateException("legacy") },
        rustEnabled = { false },
        rustPlanner = { listPlan },
      )
    }
  }

  @Test
  fun validatorRejectsVersionModuleFailureKindAndFormDrift() {
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(2, GALLERY_ARCHIVE_REQUEST_MODULE, listPlan)
        .toGalleryArchiveRequestPlan(listPlan.kind)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(1, "other", listPlan).toGalleryArchiveRequestPlan(listPlan.kind)
    }
    assertFailsWith<RustFailureException> {
      ApiEnvelope<GalleryArchiveRequestPlan>(
          1,
          GALLERY_ARCHIVE_REQUEST_MODULE,
          error = RustError(RustErrorKind.ERROR, "fixed"),
        )
        .toGalleryArchiveRequestPlan(listPlan.kind)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          1,
          GALLERY_ARCHIVE_REQUEST_MODULE,
          listPlan.copy(kind = GalleryArchiveRequestKind.DOWNLOAD),
        )
        .toGalleryArchiveRequestPlan(listPlan.kind)
    }
  }

  @Test
  fun requestConstructorRejectsEveryInvalidOptionalFieldShape() {
    val cases =
      listOf(
        Triple(GalleryArchiveRequestKind.LIST, "org", null),
        Triple(GalleryArchiveRequestKind.LIST, null, false),
        Triple(GalleryArchiveRequestKind.LIST, "org", false),
        Triple(GalleryArchiveRequestKind.DOWNLOAD, null, null),
        Triple(GalleryArchiveRequestKind.DOWNLOAD, "org", null),
        Triple(GalleryArchiveRequestKind.DOWNLOAD, null, false),
      )

    cases.forEach { (kind, resolution, isHath) ->
      assertFailsWith<IllegalArgumentException> {
        GalleryArchiveRequest(
          version = 1,
          site = GalleryUrlSite.PUBLIC,
          kind = kind,
          gid = -7,
          token = "opaque",
          resolution = resolution,
          isHath = isHath,
        )
      }
    }
  }

  @Test
  fun downloadPlansPreserveDirectAndHathFormOrderAndOpaqueValues() {
    assertEquals(
      listOf(
        GalleryFormField("dltype", "org"),
        GalleryFormField("dlcheck", "Download Original Archive"),
      ),
      downloadPlan.form,
    )
    val hath =
      GalleryArchiveRequestPlan(
        GalleryArchiveRequestKind.DOWNLOAD,
        "https://exhentai.org/archiver.php?gid=-7&token=a+b%3F",
        "https://exhentai.org/g/-7/a%20b%3F",
        "https://exhentai.org",
        listOf(GalleryFormField("hathdl_xres", "1280 weird/?")),
      )
    assertEquals(
      hath,
      ApiEnvelope(1, GALLERY_ARCHIVE_REQUEST_MODULE, hath).toGalleryArchiveRequestPlan(hath.kind),
    )
  }

  private fun ByteArray.toHex() =
    joinToString(separator = "") { byte -> byte.toUByte().toString(16).padStart(2, '0') }

  private data class WireFixture(
    val request: GalleryArchiveRequest,
    val expected: GalleryArchiveRequestPlan,
    val requestGolden: String,
    val responseGolden: String,
  )

  private companion object {
    val listRequest =
      GalleryArchiveRequest(
        1,
        GalleryUrlSite.EX_HENTAI,
        GalleryArchiveRequestKind.LIST,
        -7,
        "a b?",
      )
    val listPlan =
      GalleryArchiveRequestPlan(
        GalleryArchiveRequestKind.LIST,
        "https://exhentai.org/archiver.php?gid=-7&token=a+b%3F",
        "https://exhentai.org/g/-7/a%20b%3F",
        null,
        emptyList(),
      )
    val downloadRequest =
      GalleryArchiveRequest(
        1,
        GalleryUrlSite.PUBLIC,
        GalleryArchiveRequestKind.DOWNLOAD,
        -7,
        "a b?",
        "org",
        false,
      )
    val downloadPlan =
      GalleryArchiveRequestPlan(
        GalleryArchiveRequestKind.DOWNLOAD,
        "https://e-hentai.org/archiver.php?gid=-7&token=a+b%3F",
        "https://e-hentai.org/g/-7/a%20b%3F",
        "https://e-hentai.org",
        listOf(
          GalleryFormField("dltype", "org"),
          GalleryFormField("dlcheck", "Download Original Archive"),
        ),
      )
    val fixtures =
      listOf(
        WireFixture(listRequest, listPlan, LIST_REQUEST_CBOR_HEX, LIST_RESPONSE_CBOR_HEX),
        WireFixture(
          downloadRequest,
          downloadPlan,
          DOWNLOAD_REQUEST_CBOR_HEX,
          DOWNLOAD_RESPONSE_CBOR_HEX,
        ),
      )

    const val LIST_REQUEST_CBOR_HEX =
      "a56776657273696f6e01647369746568657848656e746169646b696e64646c697374636769642665746f6b656e646120623f"
    const val LIST_RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657567616c6c6572794172636869766552657175657374677061796c6f6164a5646b696e64646c6973746375726c783568747470733a2f2f657868656e7461692e6f72672f61726368697665722e7068703f6769643d2d3726746f6b656e3d612b622533466772656665726572782268747470733a2f2f657868656e7461692e6f72672f672f2d372f6125323062253346666f726967696ef664666f726d80656572726f72f6"
    const val DOWNLOAD_REQUEST_CBOR_HEX =
      "a76776657273696f6e016473697465667075626c6963646b696e6468646f776e6c6f6164636769642665746f6b656e646120623f6a7265736f6c7574696f6e636f726766697348617468f4"
    const val DOWNLOAD_RESPONSE_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c657567616c6c6572794172636869766552657175657374677061796c6f6164a5646b696e6468646f776e6c6f61646375726c783568747470733a2f2f652d68656e7461692e6f72672f61726368697665722e7068703f6769643d2d3726746f6b656e3d612b622533466772656665726572782268747470733a2f2f652d68656e7461692e6f72672f672f2d372f6125323062253346666f726967696e7468747470733a2f2f652d68656e7461692e6f726764666f726d82a2646e616d6566646c747970656576616c7565636f7267a2646e616d6567646c636865636b6576616c75657819446f776e6c6f6164204f726967696e616c2041726368697665656572726f72f6"
  }
}
