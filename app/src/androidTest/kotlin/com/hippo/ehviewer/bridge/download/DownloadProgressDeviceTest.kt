package com.hippo.ehviewer.bridge.download

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Offline API-35 JNI contract; execution is supplied by the connected-device job (UNRUN here). */
@RunWith(AndroidJUnit4::class)
class DownloadProgressDeviceTest {
  @Test
  fun plannerContractIsAvailableForAllCommandShapes() {
    val base =
      DownloadProgressSnapshot(
        state = DownloadState.DOWNLOADING,
        speedBytesPerSecond = 7,
        remainingMilliseconds = 9,
        finished = 1,
        downloaded = 2,
        total = 10,
        legacy = 8,
      )
    val commands =
      listOf(
        DownloadProgressCommand.reset(),
        DownloadProgressCommand.setPages(total = 12),
        DownloadProgressCommand.setPageResult(finished = 4, downloaded = 5, total = 12),
        DownloadProgressCommand.estimate(
          speedBytesPerSecond = 250,
          transfers =
            listOf(
              ActiveDownloadTransfer(contentLength = 1_000, receivedSize = 400),
              ActiveDownloadTransfer(contentLength = 500, receivedSize = 100),
            ),
        ),
        DownloadProgressCommand.finish(
          finished = 10,
          downloaded = 10,
          total = 10,
          failed = false,
        ),
        DownloadProgressCommand.finish(
          finished = 7,
          downloaded = 6,
          total = 10,
          failed = true,
        ),
      )
    val plans = commands.map { command ->
      val request = DownloadProgressRequest(version = 1, snapshot = base, command = command)
      planDownloadProgress(request)
    }

    assertEquals(22_000, plans[3].snapshot.remainingMilliseconds)
    assertEquals(DownloadState.FINISHED, plans[4].snapshot.state)
    assertEquals(DownloadState.FAILED, plans[5].snapshot.state)
  }
}
