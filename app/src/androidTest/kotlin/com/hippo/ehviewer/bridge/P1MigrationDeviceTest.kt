package com.hippo.ehviewer.bridge

import android.content.Context
import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import androidx.test.core.app.ApplicationProvider
import com.hippo.ehviewer.bridge.download.*
import com.hippo.ehviewer.bridge.gallery.*
import com.hippo.ehviewer.bridge.tags.*
import java.io.File
import org.junit.Assert.*
import org.junit.Test

/** Offline real-JNI acceptance for P1. Source compilation is not counted as device execution. */
class P1MigrationDeviceTest {
  @Test
  fun descriptorIndexIsOffsetPreservingBoundedAndIdempotentlyClosed() {
    val context = ApplicationProvider.getApplicationContext<Context>()
    val file = File.createTempFile("p1-tags", ".json", context.cacheDir)
    try {
      file.writeText("""{"f":{"aa":" 同 "}}""")
      ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY).use { descriptor ->
        Os.lseek(descriptor.fileDescriptor, 3, OsConstants.SEEK_SET)
        TagTranslationIndex.open(descriptor.fd, false).use { index ->
          assertEquals(3L, Os.lseek(descriptor.fileDescriptor, 0, OsConstants.SEEK_CUR))
          assertEquals("同", index.translate("f", "aa"))
          assertEquals(
            "f:aa",
            index.suggest(TagSuggestionRequest(1, "f:aa", true, 15)).single().tag,
          )
          index.close()
          index.close()
          assertThrows(IllegalStateException::class.java) { index.translate("f", "aa") }
        }
      }
      assertThrows(RuntimeException::class.java) { TagIndexNativeApi().open(2, -1, false) }
    } finally {
      file.delete()
    }
  }

  @Test
  fun languageAndMetadataEligibilityUseRealNativeRules() {
    assertEquals(
      "ZH",
      inferGalleryLanguage("[ENG]", listOf("language:chinese", "language:english")),
    )
    assertEquals("EN", inferGalleryLanguage("[ENG]", emptyList()))
    assertTrue(needsGalleryMetadata(GalleryMetadataFacts(true, true, false, false, false, false)))
    assertFalse(
      needsGalleryMetadata(GalleryMetadataFacts(false, false, false, false, false, false))
    )
  }

  @Test
  fun requestPlanningKeepsCredentialsOutAndMatchesFrozenTransportShapes() {
    val rate =
      planGalleryRequest(
        GalleryRequest(
          1,
          GalleryUrlSite.PUBLIC,
          GalleryRequestKind.RATE,
          gid = 7,
          token = "abc",
          rating = 2.01f,
        )
      )
    assertTrue(rate.credentials)
    assertEquals("""{"method":"rategallery","gid":7,"token":"abc","rating":5}""", rate.body)
    val page =
      planGalleryRequest(
        GalleryRequest(
          1,
          GalleryUrlSite.EX_HENTAI,
          GalleryRequestKind.PAGE_URL,
          gid = -7,
          token = "abc",
          index = Int.MAX_VALUE,
        )
      )
    assertEquals("https://exhentai.org/s/abc/-7--2147483648", page.url)
  }

  @Test
  fun recoveryLookupAndReconciliationRunWithoutHttpOrRoomEffects() {
    val lookup =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.LOOKUP,
          token = "012345abcd",
          title = "123- Name",
          displayName = "123- Name",
          hasPageToken = true,
        )
      )
    assertEquals(listOf("pageToken", "storedToken", "title", "gid"), lookup.steps.map { it.kind })
    val apply =
      planRecoveryEhLookup(
        RecoveryEhRequest(
          1,
          RecoveryEhKind.APPLY,
          gid = 123,
          token = "012345abcd",
          issues = "MISSING_TOKEN",
          evidence = "LOCAL",
          localPages = 3,
          declaredPages = 3,
          usablePath = true,
        )
      )
    assertEquals("READY", apply.status)
    assertEquals("", apply.issues)
    assertEquals("LOCAL,EH_LOOKUP", apply.evidence)
  }
}
