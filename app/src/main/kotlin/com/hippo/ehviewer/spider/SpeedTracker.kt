package com.hippo.ehviewer.spider

import android.os.SystemClock
import arrow.fx.coroutines.fixedRate
import com.hippo.ehviewer.Settings
import com.hippo.ehviewer.bridge.download.TransferSpeedHandle
import com.hippo.ehviewer.bridge.reader.ReaderPolicyKind
import com.hippo.ehviewer.bridge.reader.ReaderPolicyRequest
import com.hippo.ehviewer.bridge.reader.planReaderPolicy
import com.hippo.ehviewer.client.executeSafely
import com.hippo.ehviewer.ktor.reset
import com.hippo.ehviewer.util.LowSpeedException
import com.hippo.ehviewer.util.ensureSuccess
import io.ktor.client.plugins.ConnectTimeoutException
import io.ktor.client.plugins.onDownload
import io.ktor.client.plugins.timeout
import io.ktor.client.request.HttpRequestBuilder
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.HttpStatement
import kotlin.concurrent.atomics.AtomicReference
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.launch
import kotlinx.io.IOException

class SpeedTracker(val window: Duration = 1.seconds) : AutoCloseable {
  private val native = TransferSpeedHandle(window.inWholeNanoseconds)

  suspend fun reset() = native.reset()

  suspend fun track(bytes: Int) = native.track(SystemClock.elapsedRealtimeNanos(), bytes)

  fun speedFlow(sample: Duration = 1.seconds) =
    fixedRate(sample).mapNotNull {
      native.sample(SystemClock.elapsedRealtimeNanos())
    }

  override fun close() = native.close()
}

private typealias OnTimeout = (IOException) -> Unit

suspend inline fun <R> timeoutBySpeed(
  url: String,
  crossinline request: suspend (HttpRequestBuilder.() -> Unit) -> HttpStatement,
  crossinline l: suspend (Long, Long, Int) -> Unit,
  crossinline f: suspend (HttpResponse) -> R,
  noinline t: OnTimeout = { throw it },
) = coroutineScope {
  val onTimeout =
    AtomicReference<OnTimeout?>(t).let { { e: IOException -> it.exchange(null)?.invoke(e) } }
  val watchdog = launch {
    val timeout = 10.seconds
    delay(timeout)
    onTimeout(ConnectTimeoutException(url, timeout.inWholeMilliseconds))
  }
  val tracker = SpeedTracker(2.seconds)
  try {
    request {
      var prev = 0L
      onDownload { done, total ->
        val bytesRead = (done - prev).toInt()
        if (done == total!!) {
          tracker.reset()
        } else {
          tracker.track(bytesRead)
        }
        l(total, done, bytesRead)
        prev = done
      }
      timeout { reset() }
    }
      .executeSafely { resp ->
        watchdog.cancel()
        resp.status.ensureSuccess()
        val speedWatchdog = launch {
          val timeoutSpeed = speedLevelToSpeed(Settings.timeoutSpeed.value) * 1024L
          tracker.speedFlow().collect { speed ->
            if (speed < timeoutSpeed) {
              onTimeout(LowSpeedException(url, speed))
            }
          }
        }
        try {
          f(resp)
        } finally {
          speedWatchdog.cancel()
        }
      }
  } finally {
    tracker.close()
  }
}

const val MIN_SPEED_LEVEL = 3

fun speedLevelToSpeed(level: Int): Int =
  checkNotNull(
    planReaderPolicy(ReaderPolicyRequest(kind = ReaderPolicyKind.SPEED_LEVEL, level = level))
      .kibPerSecond
  )
