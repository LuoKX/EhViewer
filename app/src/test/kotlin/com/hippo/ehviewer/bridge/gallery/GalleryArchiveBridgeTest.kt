package com.hippo.ehviewer.bridge.gallery

import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.ArchiveDownloadTarget
import com.hippo.ehviewer.bridge.ArchiveFunds
import com.hippo.ehviewer.bridge.ArchiveListPage
import com.hippo.ehviewer.bridge.ArchiveOption
import com.hippo.ehviewer.bridge.RustError
import com.hippo.ehviewer.bridge.RustErrorKind
import com.hippo.ehviewer.bridge.RustFailureException
import com.hippo.ehviewer.bridge.decodeHex
import com.hippo.ehviewer.client.parser.Funds
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.cbor.Cbor
import kotlinx.serialization.decodeFromByteArray

class GalleryArchiveBridgeTest {
  @Test
  fun decodesAndMapsTheRustGeneratedListGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<ArchiveListPage>>(LIST_GOLDEN_CBOR_HEX.decodeHex())

    val result = envelope.toArchiveResult(externalFunds = null)

    assertEquals(1, envelope.version)
    assertEquals("galleryArchive", envelope.module)
    assertEquals(742_189, result.funds.gp)
    assertEquals(1_561_004_772, result.funds.credit)
    with(result.archiveList.single()) {
      assertEquals("org", res)
      assertEquals("", name)
      assertEquals("15.47 GiB", size)
      assertEquals("747708 GP", cost)
      assertFalse(isHath)
    }
  }

  @Test
  fun keepsAndroidOwnedFundsAndMapsHathChoices() {
    val externalFunds = Funds(gp = 12, credit = 34)
    val envelope =
      ApiEnvelope(
        version = 1,
        module = "galleryArchive",
        payload =
          ArchiveListPage(
            options =
              listOf(
                ArchiveOption(
                  resolution = "1280",
                  name = "1280x",
                  size = "1.25 GiB",
                  cost = "Free",
                  isHath = true,
                )
              )
          ),
      )

    val result = envelope.toArchiveResult(externalFunds)

    assertEquals(externalFunds, result.funds)
    assertTrue(result.archiveList.single().isHath)
  }

  @Test
  fun decodesAndValidatesTheRustGeneratedTargetGoldenFixture() {
    val envelope =
      Cbor.decodeFromByteArray<ApiEnvelope<ArchiveDownloadTarget>>(
        TARGET_GOLDEN_CBOR_HEX.decodeHex()
      )

    assertEquals("https://archive.example/file?token=a%2Fb&start=1", envelope.toArchiveUrl())
    assertEquals(
      "https://archive.example/file?st%61rt=%31",
      ApiEnvelope(
          version = 1,
          module = "galleryArchive",
          payload = ArchiveDownloadTarget("https://archive.example/file?st%61rt=%31"),
        )
        .toArchiveUrl(),
    )
    assertNull(
      ApiEnvelope(
          version = 1,
          module = "galleryArchive",
          payload = ArchiveDownloadTarget(),
        )
        .toArchiveUrl()
    )
  }

  @Test
  fun rejectsInvalidEnvelopeFundsAndOptionShapes() {
    val validPage =
      ArchiveListPage(
        options = listOf(directOption()),
        funds = ArchiveFunds(gp = 1, credit = 2),
      )
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 2, module = "galleryArchive", payload = validPage).toArchiveResult(null)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryList", payload = validPage).toArchiveResult(null)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(version = 1, module = "galleryArchive", payload = validPage)
        .toArchiveResult(Funds(3, 4))
    }
    assertFailsWith<IllegalStateException> {
      ApiEnvelope(
          version = 1,
          module = "galleryArchive",
          payload = ArchiveListPage(options = listOf(directOption())),
        )
        .toArchiveResult(null)
    }
    assertFailsWith<IllegalArgumentException> {
      ApiEnvelope(
          version = 1,
          module = "galleryArchive",
          payload =
            ArchiveListPage(
              options = listOf(directOption().copy(name = "unexpected")),
              funds = ArchiveFunds(1, 2),
            ),
        )
        .toArchiveResult(null)
    }
  }

  @Test
  fun preservesTypedFailuresAndRejectsForgedUnsafeTargets() {
    val failure = RustError(RustErrorKind.INSUFFICIENT_FUNDS)
    val listFailure =
      ApiEnvelope<ArchiveListPage>(version = 1, module = "galleryArchive", error = failure)
    assertEquals(
      failure,
      assertFailsWith<RustFailureException> { listFailure.toArchiveResult(null) }.failure,
    )

    val unsafeTargets =
      listOf(
        "http://archive.example/file?start=1",
        "https://user:secret@archive.example/file?start=1",
        "https://archive.example/file?start=1#fragment",
        "https://archive.example/file",
        "https://archive.example/file?start=1&start=1",
      )
    unsafeTargets.forEach { url ->
      assertFailsWith<IllegalArgumentException>(url) {
        ApiEnvelope(
            version = 1,
            module = "galleryArchive",
            payload = ArchiveDownloadTarget(url),
          )
          .toArchiveUrl()
      }
    }
  }

  private fun directOption() =
    ArchiveOption(
      resolution = "org",
      name = "",
      size = "1 GiB",
      cost = "100 GP",
      isHath = false,
    )

  private companion object {
    const val LIST_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e67616c6c65727941726368697665677061796c6f61" +
        "64a26b617263686976654c69737481a563726573636f7267646e616d65606473697a656931352e34" +
        "372047694264636f73746937343737303820475066697348617468f46566756e6473a26267701a00" +
        "0b532d666372656469741a5d0b0ae4656572726f72f6"
    const val TARGET_GOLDEN_CBOR_HEX =
      "a46776657273696f6e01666d6f64756c656e67616c6c65727941726368697665677061796c6f61" +
        "64a16375726c783068747470733a2f2f617263686976652e6578616d706c652f66696c653f746f6b" +
        "656e3d61253246622673746172743d31656572726f72f6"
  }
}
