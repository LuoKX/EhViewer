package com.hippo.ehviewer.bridge.download

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Real two-phase JNI policy probes; source compilation is not device runtime acceptance. */
@RunWith(AndroidJUnit4::class)
class DownloadMetadataDeviceTest {
  @Test
  fun metadataPlansPreserveComicPrecedenceDuplicatesAndConditionalSpiderFallback() {
    val request =
      DownloadMetadataRequest(
        1,
        DownloadMetadataPhase.OBSERVE,
        listOf(
          DownloadMetadataSnapshot(0, false, false, true),
          DownloadMetadataSnapshot(9, true, true, true),
          DownloadMetadataSnapshot(4, false, true, true),
          DownloadMetadataSnapshot(0, true, true, false),
        ),
        emptyList(),
      )
    assertEquals(
      listOf(
        DownloadMetadataRead(0, true),
        DownloadMetadataRead(1, false),
        DownloadMetadataRead(2, false),
      ),
      planDownloadMetadata(request).reads,
    )
    val plan =
      planDownloadMetadata(
        request.copy(
          phase = DownloadMetadataPhase.PATCH,
          observations =
            listOf(
              DownloadMetadataObservation(0, null, -1),
              DownloadMetadataObservation(
                1,
                DownloadMetadataComicFacts(7, null, listOf("a", "a")),
                null,
              ),
              DownloadMetadataObservation(2, DownloadMetadataComicFacts(99, null, null), null),
            ),
        )
      )
    assertTrue(plan.reads.isEmpty())
    assertEquals(
      listOf(
        DownloadMetadataPatch(0, -1, false, null, null, true, false),
        DownloadMetadataPatch(1, 7, true, null, listOf("a", "a"), true, true),
      ),
      plan.patches,
    )
  }

  @Test
  fun completeRowsAndMissingSidecarsProduceNoPersistenceIntent() {
    val complete =
      DownloadMetadataRequest(
        1,
        DownloadMetadataPhase.OBSERVE,
        listOf(DownloadMetadataSnapshot(2, false, false, true)),
        emptyList(),
      )
    assertTrue(planDownloadMetadata(complete).reads.isEmpty())
    val missing =
      complete.copy(
        phase = DownloadMetadataPhase.PATCH,
        entries = listOf(DownloadMetadataSnapshot(0, true, true, true)),
        observations = listOf(DownloadMetadataObservation(0, null, null)),
      )
    assertTrue(planDownloadMetadata(missing).patches.isEmpty())
  }
}
