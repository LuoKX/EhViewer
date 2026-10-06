package com.hippo.ehviewer.bridge.archive

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real scalar JNI probes; compiling these sources is not device runtime acceptance. */
@RunWith(AndroidJUnit4::class)
class ComicInfoPolicyDeviceTest {
  @Test
  fun preparePlansUseExpectedCountAndKeepExistingMetadataDistinctFromCreation() {
    val initial = ComicInfoPolicyRequest.prepare(2, false, true)
    assertEquals(ComicInfoAction.READ_METADATA, planComicInfoPolicy(initial).action)
    val create = planComicInfoPolicy(initial.copy(metadataState = ComicInfoMetadataState.MISSING))
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.CREATE, 2, ComicInfoFetchMode.CONTINUE),
      create,
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.REWRITE, 2, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(
        initial.copy(metadataState = ComicInfoMetadataState.PRESENT, metadataPages = 1)
      ),
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.KEEP, 2, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(
        initial.copy(metadataState = ComicInfoMetadataState.PRESENT, metadataPages = 2)
      ),
    )
  }

  @Test
  fun exportAndDirectWritePreserveLazyReadsAndRefreshModes() {
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.RESOLVE_PAGES, 5, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(ComicInfoPolicyRequest.resolveExport(5, false)),
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.RESOLVE_PAGES, 7, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(
        ComicInfoPolicyRequest.resolveExport(0, true)
          .copy(metadataState = ComicInfoMetadataState.PRESENT, metadataPages = 7)
      ),
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.CREATE, null, ComicInfoFetchMode.PROPAGATE),
      planComicInfoPolicy(ComicInfoPolicyRequest.write(true, false, true)),
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.CREATE, null, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(ComicInfoPolicyRequest.write(true, true, true)),
    )
    assertEquals(
      ComicInfoPolicyPlan(ComicInfoAction.SKIP, null, ComicInfoFetchMode.NONE),
      planComicInfoPolicy(ComicInfoPolicyRequest.write(false, false, true)),
    )
  }

  @Test
  fun legacyValidationErrorsAreReturnedByRealRust() {
    assertEquals(
      "Archive expected page count must be positive",
      assertThrows(IllegalArgumentException::class.java) {
          planComicInfoPolicy(ComicInfoPolicyRequest.prepare(0, false, true))
        }
        .message,
    )
    assertEquals(
      "Required value was null.",
      assertThrows(IllegalArgumentException::class.java) {
          planComicInfoPolicy(ComicInfoPolicyRequest.prepare(2, false, false))
        }
        .message,
    )
  }
}
