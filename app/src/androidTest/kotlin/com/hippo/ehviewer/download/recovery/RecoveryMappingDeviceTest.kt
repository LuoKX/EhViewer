package com.hippo.ehviewer.download.recovery

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.hippo.ehviewer.bridge.download.RecoveryMappingCandidate
import com.hippo.ehviewer.bridge.download.RecoveryMappingDownload
import com.hippo.ehviewer.bridge.download.RecoveryMappingRequest
import com.hippo.ehviewer.bridge.download.RecoveryStatus
import com.hippo.ehviewer.bridge.download.planRecoveryMappings
import com.hippo.ehviewer.bridge.files.toUtf16LeBytes
import java.security.MessageDigest
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class RecoveryMappingDeviceTest {
  @Test
  fun rawPathEqualityAndPlatformUtf8HashesThroughRealJni() {
    val paths = listOf(null, "", "x", "X", "😀", "\ud800", "\udc00", "?", "😀\ud800\ud800")
    val candidates = paths.flatMap { path ->
      listOf(
        RecoveryMappingCandidate(
          7,
          RecoveryStatus.CONFLICT,
          "0".repeat(64),
          path?.toUtf16LeBytes(),
        ),
        RecoveryMappingCandidate(7, RecoveryStatus.READY, "a".repeat(64), path?.toUtf16LeBytes()),
        RecoveryMappingCandidate(7, RecoveryStatus.READY, "b".repeat(64), path?.toUtf16LeBytes()),
      )
    }
    val downloads = listOf(7L, 8L, Long.MIN_VALUE).flatMap { gid -> paths.map { gid to it } }
    val result =
      planRecoveryMappings(
        RecoveryMappingRequest(
          1,
          candidates,
          downloads.map { (gid, path) -> RecoveryMappingDownload(gid, path?.toUtf16LeBytes()) },
        )
      )
    downloads.forEachIndexed { index, (gid, path) ->
      val first =
        candidates
          .indexOfFirst {
            it.status == RecoveryStatus.READY &&
              it.gid == gid &&
              it.pathUtf16.contentEquals(path?.toUtf16LeBytes())
          }
          .takeIf { it >= 0 }
      assertEquals(first, result.mappings[index].candidateIndex)
      val expected =
        first?.let { candidates[it].sourceKey }
          ?: MessageDigest.getInstance("SHA-256")
            .digest("database:$gid:${path.orEmpty()}".encodeToByteArray())
            .joinToString("") { "%02x".format(it) }
      assertEquals(expected, result.mappings[index].sourceKey)
    }
  }
}
