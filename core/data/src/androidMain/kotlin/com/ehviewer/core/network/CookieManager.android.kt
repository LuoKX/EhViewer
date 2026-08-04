package com.ehviewer.core.network

import io.ktor.http.Cookie
import io.ktor.http.Url
import io.ktor.http.parseClientCookiesHeader
import io.ktor.http.renderSetCookieHeader
import kotlin.coroutines.resume
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

class AndroidCookieManager : CookieManager {
  private val manager = android.webkit.CookieManager.getInstance()

  override fun getCookies(url: Url) =
    manager.getCookie(url.toString())?.let { parseClientCookiesHeader(it) }

  override fun setCookie(url: Url, cookie: Cookie) =
    manager.setCookie(url.toString(), renderSetCookieHeader(cookie))

  override suspend fun removeAllCookies() =
    withContext(NonCancellable) {
      withContext(Dispatchers.Main.immediate) {
        suspendCancellableCoroutine { continuation ->
          manager.removeAllCookies {
            if (continuation.isActive) continuation.resume(Unit)
          }
        }
      }
      withContext(Dispatchers.IO) { manager.flush() }
    }

  override fun flush() = manager.flush()
}

actual fun getCookieManager(): CookieManager = AndroidCookieManager()
