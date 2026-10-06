package com.hippo.ehviewer.bridge.updater

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.hippo.ehviewer.bridge.ApiEnvelope
import com.hippo.ehviewer.bridge.rustCbor
import java.io.IOException
import kotlinx.serialization.decodeFromByteArray
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith

/** Offline real JNI probes. Source compilation is not device runtime acceptance. */
@RunWith(AndroidJUnit4::class)
class UpdaterDeviceTest {
  @Test
  fun sharedRequestResponseGoldensUseRealNativeBoundaryWithoutHttp() {
    val assets = InstrumentationRegistry.getInstrumentation().context.assets
    val names = checkNotNull(assets.list("updater")).filter { it.endsWith("_request.cbor.hex") }
    assertEquals(27, names.size)
    for (name in names) {
      fun bytes(path: String) =
        assets
          .open("updater/$path")
          .bufferedReader()
          .use {
            it.readText()
          }
          .trim()
          .chunked(2)
          .map { it.toInt(16).toByte() }
          .toByteArray()
      val request = rustCbor.decodeFromByteArray<UpdateRequest>(bytes(name))
      val response =
        rustCbor.decodeFromByteArray<ApiEnvelope<UpdatePlan>>(
          bytes(name.replace("_request", "_response"))
        )
      if (request.version != 1 || request.policy.version != 1) {
        assertThrows(IllegalArgumentException::class.java) { planUpdate(request) }
      } else if (response.error != null) {
        assertEquals(
          response.error.message,
          assertThrows(IOException::class.java) {
              planUpdate(request)
            }
            .message,
        )
      } else {
        assertEquals(response.payload, planUpdate(request))
      }
    }
  }

  @Test
  fun customPredicateKeepsUtf16OrderAndStopsImmediatelyAtFirstMatch() {
    val visited = mutableListOf<String>()
    assertEquals(
      1,
      selectUpdateBinary(
        listOf("\ue000", "😀", "😀"),
        true,
        {
          visited.add(it)
          true
        },
      ),
    )
    assertEquals(listOf("😀"), visited)
    assertEquals(
      UpdateDecision.CHECK,
      planUpdatePolicy(UpdatePolicyRequest(operation = UpdateOperation.CHECK, forceCheck = true))
        .decision,
    )
  }
}
