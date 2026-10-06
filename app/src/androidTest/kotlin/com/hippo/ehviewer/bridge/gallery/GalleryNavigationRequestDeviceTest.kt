package com.hippo.ehviewer.bridge.gallery

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import com.hippo.ehviewer.client.EhUrl
import kotlinx.serialization.decodeFromByteArray
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real-JNI URLs; deliberately no browser, HTTP, settings or credential effects. */
@RunWith(AndroidJUnit4::class)
class GalleryNavigationRequestDeviceTest {
  @Test
  fun remainingNavigationKindsMatchSharedWireGoldens() {
    val context = ApplicationProvider.getApplicationContext<Context>()

    fun bytes(
      name: String,
      suffix: String,
    ) =
      context.assets
        .open("navigation_${name}_$suffix.cbor.hex")
        .bufferedReader()
        .use { it.readText() }
        .trim()
        .chunked(2)
        .map { it.toInt(16).toByte() }
        .toByteArray()
    for (name in
      listOf(
        "favorites",
        "torrent",
        "tracker_null",
        "tracker_empty",
        "tracker_key",
        "wiki",
        "wiki_high",
        "wiki_low",
        "wiki_pair_high",
        "wiki_pair_low",
        "wiki_delimiters_space",
      )) {
      val request = rustCbor.decodeFromByteArray<GalleryRequest>(bytes(name, "request"))
      val expected =
        rustCbor
          .decodeFromByteArray<ApiEnvelope<GalleryRequestPlan>>(bytes(name, "response"))
          .payload
      val actual = planGalleryRequest(request)
      val raw = expected?.wikiUrlUtf16
      if (raw == null) assertEquals(expected, actual)
      else {
        val units =
          CharArray(raw.size / 2) {
            ((raw[it * 2].toInt() and 255) or ((raw[it * 2 + 1].toInt() and 255) shl 8)).toChar()
          }
        assertEquals(units.toList(), actual.url.toCharArray().toList())
        assertEquals(
          expected.copy(url = actual.url, wikiUrlUtf16 = null, wikiSourceIndex = null),
          actual,
        )
      }
    }
  }

  @Test
  fun publicWikiHelperMatchesFrozenJavaStringOracleForLoneUnits() {
    for (tag in
      listOf(
        "\uD800",
        "\uDC00",
        "\uD83D\uDE00\uD800",
        "\uD83D\uDE00\uDC00",
        " a/+%#?\uD800 \uDC00 ",
      )) {
      val oracle = "https://ehwiki.org/wiki/" + tag.replace(' ', '_')
      assertEquals(
        oracle.toCharArray().toList(),
        EhUrl.getTagDefinitionUrl(tag).toCharArray().toList(),
      )
    }
  }
}
