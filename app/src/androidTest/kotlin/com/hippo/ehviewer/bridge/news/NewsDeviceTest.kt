package com.hippo.ehviewer.bridge.news

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.client.parser.EventPaneParser
import java.nio.ByteBuffer
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/** Real-JNI device contract for the independent News event-pane rollout. */
@RunWith(AndroidJUnit4::class)
class NewsDeviceTest {
  @Test
  fun parsesTheCanonicalFixtureThroughTheVersionedNativeBoundary() = runBlocking {
    val previous = Settings.useRustCore.value
    try {
      Settings.withMutableSnapshot { snapshot -> snapshot[Settings.useRustCore] = true }

      assertEquals(
        "<strong>Event &amp; reward</strong>",
        EventPaneParser.parse(assetBuffer("news.html")),
      )
    } finally {
      Settings.withMutableSnapshot { snapshot -> snapshot[Settings.useRustCore] = previous }
    }
  }

  private fun assetBuffer(name: String): ByteBuffer {
    val bytes =
      InstrumentationRegistry.getInstrumentation().context.assets.open(name).use { it.readBytes() }
    return ByteBuffer.allocateDirect(maxOf(bytes.size * 2, MINIMUM_BUFFER_CAPACITY)).apply {
      put(bytes)
      flip()
    }
  }

  private companion object {
    const val MINIMUM_BUFFER_CAPACITY = 64 * 1024
  }
}
